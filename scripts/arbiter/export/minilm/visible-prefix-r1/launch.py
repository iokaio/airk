# SPDX-License-Identifier: Apache-2.0
"""One bounded CPU export window, complete retained runtime and private source pins."""
from pathlib import Path
import hashlib
import importlib.util
import json
import os
import sys
import time
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
HERE=Path(__file__).resolve().parent
MODELS=ROOT/'artifacts/advanced-split/models'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    old=MODELS/'a1-visible-prefix-training-execution-r1/training-execution-seal.json'
    if sha(old)!='57124cb5ca8321729318e60d234a0a19b56fea2f4b18705bad7daabbecbb5cff':raise ValueError('TRAIN runtime seal changed')
    original=json.loads(old.read_bytes())
    # Explicit new stage: preserve full runtime/driver identity; replace prior
    # public tools closure with reviewed private export source closure. This is
    # neither a TRAIN reseal nor permission to mutate historical TRAIN evidence.
    public_tools=ROOT/'tools/retrieval-train'
    rows={r['path']:r for r in original['files'] if not Path(r['path']).is_relative_to(public_tools)}
    added=list(HERE.glob('*.py'))+list((MODELS/'minilm-visible-prefix-mining-r1/epoch-1').iterdir())
    added += [MODELS/'minilm-visible-prefix-mining-r1/receipt.json',HERE/'adaptation.json',ROOT/'tools/retrieval-train/training_execution.py',ROOT/'target/local-vector-model/minilm-l6-v2-u8-v1/LICENSE']
    for path in added:
        if path.is_file():rows[str(path)]={'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size}
    for row in rows.values():
        if sha(row['path'])!=row['sha256']:raise ValueError('runtime/source drift before export: '+row['path'])
    for name,pin in {'model.safetensors':'f0c31fc04f2e516dba7221518790a71fa42c9c4088343327562c1661c97b9edf','config.json':'1bed8d6657f4e2f14c1922753b36d1dface054e834a33d4e97ea1d437a094dcc','tokenizer.json':'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037'}.items():
        if sha(MODELS/'minilm-visible-prefix-mining-r1/epoch-1'/name)!=pin:raise ValueError('strict epoch1 source mismatch')
    work=MODELS/'visible-prefix-export-execution-r1'
    work.mkdir(exist_ok=False)
    seal={'schema':'airk.private-selected-epoch1-export-seal/1','TRAIN_parent_sha256':sha(old),'files':list(rows.values()),'limits':{'whole_seconds':600,'aggregate_host_bytes':8*1024**3,'job_processes':2,'CPU_only':True},'runtime_scope':'Full qualified TRAIN runtime/stdlib/platform/driver closure retained. Public TRAIN tools tree replaced by independently reviewed private export source tree; no training resumed.'}
    seal_path=work/'seal.json';seal_path.write_text(json.dumps(seal,indent=2)+'\n');seal_sha=sha(seal_path)
    argv=[sys.executable,'-B',str(HERE/'stage.py'),str(seal_path),seal_sha]
    environment=dict(os.environ);environment.update({'PYTHONDONTWRITEBYTECODE':'1','CUDA_VISIBLE_DEVICES':'-1','OMP_NUM_THREADS':'2','MKL_NUM_THREADS':'2','OPENBLAS_NUM_THREADS':'2'})
    owner=ROOT/'tools/retrieval-train/training_execution.py'
    if sha(owner)!='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975':raise ValueError('owned launcher changed')
    spec=importlib.util.spec_from_file_location('owned',owner);owned=importlib.util.module_from_spec(spec);spec.loader.exec_module(owned)
    started=time.monotonic()
    (work/'stdin.txt').write_bytes(b'')
    with (work/'stdin.txt').open('rb') as stdin,(work/'stdout.log').open('wb') as log:
        result=owned.run_windows_owned_training(argv,ROOT,log,600,environment,stdin,8*1024**3)
    receipt={'argv':argv,'seal_sha256':seal_sha,'owned':result,'elapsed_seconds':time.monotonic()-started,'CPU_only':True,'environment_overrides':{k:environment[k] for k in ('PYTHONDONTWRITEBYTECODE','CUDA_VISIBLE_DEVICES','OMP_NUM_THREADS','MKL_NUM_THREADS','OPENBLAS_NUM_THREADS')}}
    (work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled']:raise ValueError('owned export failed; preserve all output/no automatic retry')
    print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
