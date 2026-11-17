# SPDX-License-Identifier: Apache-2.0
"""Prepare an additive seal; never release or execute a model trial."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path.cwd()
HERE=Path(__file__).resolve().parent
BASE=ROOT/'artifacts/advanced-split/models/encoder-recovery-preparation-r1/execution-seal-train-r3.json'
BASE_SHA='fdfedb8a858eb5083a6bc182cab4be75360be15d46b5c57025017f1f5eefcf8a'
def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def row(path):
    path=Path(path).resolve()
    return dict(path=str(path),sha256=digest(path),bytes=path.stat().st_size)
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists():raise ValueError('Preserve earlier seal; fresh output required')
    if digest(BASE)!=BASE_SHA:raise ValueError('Successful R3 base changed')
    seal=json.loads(BASE.read_bytes());original=list(seal['files'])
    source={name:digest(HERE/name) for name in ['run_focus_export.py','focus_reference.py','owned_reference.py']}
    release=json.loads(args.release.read_bytes())
    if release['action']!='focus-publisher-reference' or release['source_sha256']!=source or release['limits']!=seal['limits']:
        raise ValueError('Reference release source/limits mismatch')
    seal['status']='FOCUS_REFERENCE_PENDING_ROOT_RELEASE' if release['status']!='ROOT_RELEASED' else 'ROOT_RELEASED_FOCUS_REFERENCE_ONLY'
    cache=str(HERE/'disabled-pycache-reference-r1')
    seal['environment'].update(PYTHONPYCACHEPREFIX=cache,OMP_NUM_THREADS='1',MKL_NUM_THREADS='1')
    seal['launch_log_dir']=str(HERE/'owned-reference-r1')
    seal['focus_output']=str(HERE/'reference-r1')
    seal['command']=[seal['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+cache,str(HERE/'owned_reference.py'),
        '--execution-seal',str(args.output.resolve()),'--execution-seal-sha256','<SEAL_SHA256>',
        '--primary-release',str(args.release.resolve()),'--primary-release-sha256',digest(args.release)]
    seal['focus_reference_amendment']={'base_seal_sha256':BASE_SHA,'original_file_rows':len(original),'no_original_pin_removed_or_replaced':True,
        'scope':'Six actual publisher/full-trained-head vs shared-backbone/two-row controls only; no native consumer, quality, hardware tier or export qualification'}
    # Importing this module loads no Torch, Transformers or weights.
    import run_focus_export
    additions=[row(path) for path in run_focus_export.verify_assets()]
    additions += [row(HERE/name) for name in ['run_focus_export.py','focus_reference.py','owned_reference.py','prepare_reference_owner.py','reference-export-freeze.json']]
    additions += [row(args.release),row(BASE)]
    indexed={item['path'].lower():item for item in original}
    for item in additions:
        old=indexed.get(item['path'].lower())
        if old and old!=item:raise ValueError('Refuse replacing inherited runtime/source pin')
        if not old:seal['files'].append(item);indexed[item['path'].lower()]=item
    if seal['files'][:len(original)]!=original:raise ValueError('Original pin bytes changed')
    args.output.write_text(json.dumps(seal,indent=2)+'\n',encoding='utf8')
    print(json.dumps({'seal':str(args.output),'sha256':digest(args.output),'original_rows':len(original),'added_rows':len(seal['files'])-len(original),'root_released':release['status']=='ROOT_RELEASED'}))
if __name__=='__main__':main()
