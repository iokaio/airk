# SPDX-License-Identifier: Apache-2.0
"""Prepare a NEW Focus artifact with exact-value native-compatible ONNX storage; no NN."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import time
from storage_normalizer import normalize_external_storage,sha

HERE=Path(__file__).resolve().parent
SOURCE=HERE.parent/'qwen3-reranker06-weight8-r1/weight8-r1'
NORMALIZER_SHA='67df2aea347438c5872fd7e98c7f4c8833ca0ff4783b5ed3ff9e061ce2051e12'

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();out=args.output.resolve();started=time.monotonic()
    if out.exists()or not out.is_relative_to(HERE.parent):raise ValueError('Fresh private output under campaign models required')
    if sha(HERE/'storage_normalizer.py')!=NORMALIZER_SHA:raise ValueError('Reviewed normalizer source differs')
    prior=json.loads((SOURCE/'receipt.json').read_bytes())
    if prior.get('complete')is not True or len(prior['rows'])!=12:raise ValueError('Complete original weight8 numerical receipt required')
    expected={a['name']:a for a in prior['graph_closure']['files']}
    if set(expected)!={'model.onnx','model.onnx.data'}:raise ValueError('Exact original two-file graph closure required')
    pins={str(SOURCE/'receipt.json'):sha(SOURCE/'receipt.json'),str(HERE/'storage_normalizer.py'):NORMALIZER_SHA,str(Path(__file__).resolve()):sha(__file__)}
    for name,asset in expected.items():
        path=SOURCE/name
        if path.is_symlink()or path.stat().st_size!=asset['bytes']or sha(path)!=asset['sha256']:raise ValueError('Original failed artifact differs')
        pins[str(path)]=asset['sha256']
    out.mkdir();model=out/'model';model.mkdir()
    receipt={'schema':'airk.private-focus-native-storage-stage/1','complete':False,'input_pins':pins,'model_executed':False,'quality_or_release_qualified':False}
    try:
        for name in expected:shutil.copyfile(SOURCE/name,model/name)
        for name,asset in expected.items():
            if sha(model/name)!=asset['sha256']:raise ValueError('Independent artifact copy differs')
        proof=normalize_external_storage(model/'model.onnx',original_graph_path=out/'original-model.onnx',proof_path=out/'storage-proof.json')
        if len(proof['inlined'])!=3 or sum(r['length']for r in proof['inlined'])!=1056768 or proof['remaining_external_main_tensors']!=451:
            raise ValueError('Actual known Focus storage-class census differs')
        if sha(model/'model.onnx.data')!=expected['model.onnx.data']['sha256']:raise ValueError('Main trained weight file changed')
        if any(sha(path)!=expected_hash for path,expected_hash in pins.items()):raise ValueError('Original source drift during normalization')
        receipt.update(complete=True,proof_sha256=sha(out/'storage-proof.json'),model_files=[{'name':p.name,'bytes':p.stat().st_size,'sha256':sha(p)}for p in sorted(model.iterdir())])
    except BaseException as error:
        receipt.update(error_type=type(error).__name__,error=str(error));raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-started
        (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'complete':receipt['complete'],'model_executed':False,'elapsed_seconds':receipt['elapsed_seconds'],'error':receipt.get('error')}),flush=True)
if __name__=='__main__':main()
