# SPDX-License-Identifier: Apache-2.0
"""Retain candidate artifact/source metadata only; never grades DEV or final."""
from pathlib import Path
import hashlib
import json
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
MODELS=ROOT/'artifacts/advanced-split/models'
def pin(path):
    with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
    return {'path':str(path.relative_to(ROOT)),'sha256':digest,'bytes':path.stat().st_size}
paths=[MODELS/'minilm-visible-prefix-mining-r1/receipt.json',MODELS/'minilm-visible-prefix-mining-r1/epoch-1/model.safetensors',MODELS/'minilm-visible-prefix-mining-r1/epoch-1/config.json',MODELS/'a1-visible-prefix-mining-candidate-r1/candidate-training-rows.private.json',MODELS/'a1-visible-prefix-mining-candidate-r1/candidate-protocol.json',MODELS/'a1-visible-prefix-training-execution-r1/training-execution-seal.json',MODELS/'visible-prefix-export-execution-r1/seal.json',MODELS/'visible-prefix-export-execution-r1/execution.json',MODELS/'visible-prefix-mining-exports-r1/receipt.json',MODELS/'visible-prefix-mining-exports-r1/stage-receipt.json',MODELS/'visible-prefix-mining-native-inputs-r1/receipt.json',MODELS/'visible-prefix-mining-eager-reference-r1/receipt.json']
for short in ('u8','s8'):
    paths.extend(MODELS/'visible-prefix-mining-exports-r1'/short/name for name in ('model.onnx','tokenizer.json','LICENSE'))
paths.extend(p for p in Path(__file__).parent.iterdir() if p.suffix=='.py')
out=MODELS/'visible-prefix-mining-candidate-artifacts-r1.json'
if out.exists():raise ValueError('fresh immutable artifact manifest required')
out.write_text(json.dumps({'schema':'airk.private-epoch1-encoder-candidate-artifacts/1','status':'CPU_NUMERICAL_PASS_NATIVE_ADMISSION_PENDING','epoch':1,'checkpoint_selection':'Fixed before any new DEV score; epochs2/3 not candidates','files':[pin(p) for p in paths],'families':['visible-prefix-mining-r1-u8','visible-prefix-mining-r1-s8'],'numeric':{'cases':12,'buckets':[32,64,128,256],'min_cosine':.9955635722950834,'max_l2':.09419583541660971,'eager_fp32_max_component_error':1.3606119368680947e-7},'native':'Rust typed manifests/master and actual NativeInferenceOwner numeric/DEV capture pending; product catalog unchanged','scientific':'Original205 FAILED unchanged; new DEV development only; no CAL/FINAL/model-selection exposure; final floors/composition unchanged','timing':'CPU export ran concurrently with native baseline/compiler; no latency/resource qualification'},indent=2)+'\n')
print(json.dumps(pin(out)))
