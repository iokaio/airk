# SPDX-License-Identifier: Apache-2.0
"""CPU-only selected epoch1 export and unchanged independent numerical gates."""
from pathlib import Path
import hashlib
import json
import os
import runpy
import sys
import time

ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
HERE=Path(__file__).resolve().parent
MODELS=ROOT/'artifacts/advanced-split/models'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def verify(seal):
    for row in seal['files']:
        if sha(row['path'])!=row['sha256']:raise ValueError('export stage source/runtime/input drift: '+row['path'])
def main():
    if os.environ.get('CUDA_VISIBLE_DEVICES')!='-1':raise ValueError('reviewed CPU-only export environment required')
    seal_path=Path(sys.argv[1]);seal_sha=sys.argv[2]
    if sha(seal_path)!=seal_sha:raise ValueError('export seal changed')
    seal=json.loads(seal_path.read_bytes());verify(seal)
    checkpoint=MODELS/'native-parent-bag-training-anchor-r2/result/epoch-1'
    exports=MODELS/'native-parent-bag-exports-r1'
    oracle=MODELS/'native-parent-bag-native-inputs-r1'
    eager=MODELS/'native-parent-bag-eager-reference-r1'
    started=time.monotonic()
    def run(source,args):
        sys.argv=[str(HERE/source),*map(str,args)]
        runpy.run_path(str(HERE/source),run_name='__main__')
    run('runner.py',['--checkpoint',checkpoint,'--corpus',MODELS/'human-corpus-r1/corpus.private.json','--documentation',MODELS/'requests-train-rights/pairs.private.json','--output',exports])
    run('verify_encoder_inputs.py',['--exports',exports,'--output',oracle])
    run('verify_encoder_fp32.py',['--checkpoint',checkpoint,'--exports',exports,'--oracle',oracle/'receipt.json','--output',eager])
    numerical=json.loads((oracle/'receipt.json').read_bytes())
    if len(numerical['cases'])!=12 or {c['bucket'] for c in numerical['cases']}!={32,64,128,256}:raise ValueError('all12 independent bucket cases required')
    for case in numerical['cases']:
        for result in case['results'].values():
            if not result['finite'] or result['cosine']<.995 or result['l2']>.1:raise ValueError('unchanged candidate numeric admission failed: '+case['name'])
    verify(seal)
    result={'schema':'airk.private-selected-epoch1-export-stage/1','training_seal_sha256':'fdfedb8a858eb5083a6bc182cab4be75360be15d46b5c57025017f1f5eefcf8a','epoch1_sha256':'e4d2c66590e830cc9fedde5061e86a5195891db490a7eb967e3ec9c52c566d29','execution_seal_sha256':seal_sha,'CPU_only':True,'elapsed_seconds':time.monotonic()-started,'cases':12,'export_receipt_sha256':sha(exports/'receipt.json'),'oracle_receipt_sha256':sha(oracle/'receipt.json'),'eager_receipt_sha256':sha(eager/'receipt.json'),'native_manifest':'PENDING actual Rust typed emission/validation; no native quality claim'}
    (exports/'stage-receipt.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result),flush=True)
if __name__=='__main__':main()
