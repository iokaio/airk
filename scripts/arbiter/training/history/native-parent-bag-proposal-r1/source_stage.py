# SPDX-License-Identifier: Apache-2.0
"""One whole60s bounded source-only conversion with full pre/post seals."""
from pathlib import Path
import hashlib
import json
import sys
import time
import types
HERE=Path(__file__).resolve().parent
def sha(raw):return hashlib.sha256(raw).hexdigest()
def main():
    started=time.monotonic();path=Path(sys.argv[1]);expected=sys.argv[2];raw=path.read_bytes()
    if sha(raw)!=expected:raise ValueError('source-stage review packet changed')
    review=json.loads(raw);pins={row['path']:row['sha256'] for row in review['inputs']}
    runtime_path=HERE/'source_runtime.py';runtime_raw=runtime_path.read_bytes()
    if sha(runtime_raw)!=pins[str(runtime_path)]:raise ValueError('runtime verifier source changed')
    runtime=types.ModuleType('_verified_source_runtime');runtime.__file__=str(runtime_path);exec(compile(runtime_raw,str(runtime_path),'exec'),runtime.__dict__)
    count=runtime.verify(review,started)
    command=review['source_command'];script=Path(command[-3]);source=script.read_bytes()
    if sha(source)!=pins[str(script)]:raise ValueError('source converter changed')
    # Execute the exact verified converter bytes, not a second path open.
    module=types.ModuleType('__main__');module.__file__=str(script);sys.argv=[str(script),*command[-2:]]
    exec(compile(source,str(script),'exec'),module.__dict__)
    runtime.verify(review,started)
    output=Path(command[-1]);manifest=output/'manifest.private.json'
    receipt={'schema':'airk.private-native-TRAIN-source-preparation/1','review_sha256':expected,'verified_files_pre_and_post':count,'manifest_sha256':runtime.sha(manifest),'elapsed_seconds':time.monotonic()-started,'no_model_or_native_execution':True,'no_gradients':True}
    (output/'operation-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
