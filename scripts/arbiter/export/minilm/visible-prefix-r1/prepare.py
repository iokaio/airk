# SPDX-License-Identifier: Apache-2.0
"""Mechanical candidate identity/provenance adapter; numerical recipe preserved."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"support"))
from recipe_paths import private_run_root, fresh_output
import hashlib
import json
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
SOURCE_DIR=Path(__file__).resolve().parent;HERE=private_run_root(ROOT)
source=ROOT/'tools/retrieval-train'
for name in ('safe_int8.py','training_recipe.py','training_recipe_audit.py','runner.py','verify_encoder_inputs.py','verify_encoder_fp32.py','adaptation.json'):fresh_output(HERE,HERE/name)
text=(source/'export_encoder.py').read_text()
changes=[]
def replace(old,new):
    global text
    if text.count(old)!=1:raise ValueError('nonunique adaptation: '+old[:80])
    text=text.replace(old,new);changes.append({'old':old,'new':new})
replace('ROOT=Path(__file__).resolve().parents[2]', 'ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())\nfrom training_recipe import train_only_corpus\nfrom training_recipe_audit import _field')
replace("args=parser.parse_args();output=args.output.resolve()", "args=parser.parse_args();output=args.output.resolve()\n    if args.baseline_original:raise ValueError('candidate-only exporter')")
replace("    model=BertModel(", "    if sha(retained[args.checkpoint/'model.safetensors'])!='f0c31fc04f2e516dba7221518790a71fa42c9c4088343327562c1661c97b9edf':raise ValueError('only preselected epoch1 permitted')\n    model=BertModel(")
replace("corpus=json.loads(retained[args.corpus]);documentation=json.loads(retained[args.documentation]);train_groups=", "corpus=train_only_corpus(retained[args.corpus]);corpus['qasper_rights_decisions']=json.loads(_field(retained[args.corpus],'qasper_rights_decisions'));documentation=json.loads(retained[args.documentation]);train_groups=")
start=text.index("        manifest={'schema':2")
end=text.index("        profiles.append(",start)
removed=text[start:end]
text=text[:start]+text[end:]
changes.append({'removed_native_manifest_construction':removed})
replace("'minilm-l6-v2-'+short+('-original-mixed-eval-v1' if args.baseline_original else '-v2')", "'visible-prefix-mining-r1-'+short")
replace("'manifest':manifest,", "'native_manifest_pending_Rust_owner':True,")
replace("p['manifest']['model_sha256']", "next(a['sha256'] for a in p['assets'] if a['name']=='model.onnx')")
replace('V2profileproposal only, not catalog/default/index identity migration or quality/native/platform/package qualification.', 'Distinct visible-prefix-mining-r1 candidate only; epoch1 preselected. Rust owns final OnnxManifest and native admission. No DEV quality/platform/package claim.')
for name in ('safe_int8.py','training_recipe.py','training_recipe_audit.py'):
    (HERE/name).write_bytes((source/name).read_bytes())
(HERE/'runner.py').write_text(text)
for name in ('verify_encoder_inputs.py','verify_encoder_fp32.py'):
    original=(source/name).read_text()
    (HERE/name).write_text(original.replace('ROOT=Path(__file__).resolve().parents[2]','ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())'))
(HERE/'adaptation.json').write_text(json.dumps({'original_sha256':hashlib.sha256((source/'export_encoder.py').read_bytes()).hexdigest(),'candidate_sha256':hashlib.sha256((HERE/'runner.py').read_bytes()).hexdigest(),'changes':changes,'scope':'Numerical export only; no native DTO mirror/model.json; strict epoch1 pin, TRAIN-only selective loader, unchanged FP32/quantization/12-case helper.'},indent=2)+'\n')
compile(text,str(HERE/'runner.py'),'exec')
print('Prepared candidate exporter; no NN executed')
