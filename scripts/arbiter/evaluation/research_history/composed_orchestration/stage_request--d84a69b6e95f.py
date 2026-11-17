# SPDX-License-Identifier: Apache-2.0
"""Stage opaque native /2 metadata request only. Never spawn/emits/forward."""
from pathlib import Path
import argparse,copy,hashlib,json,os
BASE=Path(__file__).resolve().parent
ROOT=BASE.parent
REPO=ROOT.parents[3]
ORIGINAL=ROOT/'final-plan-current-policy-preparation-20261116T085632Z/metadata-watchdog-b2-r4/metadata-request.private.json'
ORIGINAL_SHA='1c9488b4f0ca92d159c22537aeea0e5d39047cfcdd0044c35b24a5f344fdd6d3'
BASELINE=ROOT/'final-plan-current-policy-preparation-20261116T085632Z/metadata-watchdog-b2-r4/arbiter-context-qwen3-17b/numeric-resource-run-input.private.json'
BASELINE_SHA='75f890626eaa87189f498461272bcc38ea6e2542540952da5c49c2a6d96923cf'
FIXED={
 'graph':(ROOT/'compact-gqa-graph-r3/model.onnx','d2b32177c875a989bdbe56071168bbcabb8d589ba740099feea44972459828c3'),
 'shape_proof':(ROOT/'compact-gqa-graph-r3/model.proof.json','7cd056abdedffe07a4c8684de3b21744e4f877630d7b1415f3b83aa292b249f0'),
 'owned_creation':(ROOT/'compact-gqa-graph-r3/owned-execution.json','cc5f46ff48e7d343814d4d7c10085c8de8f19224e381b2b111baabb80587b99a'),
 'baseline_input':(BASELINE,BASELINE_SHA),
}
ADDED_SOURCES=[
 'crates/airk-retrieval/src/arbiter/runtime.rs',
 'crates/airk-retrieval/src/arbiter/runtime/input.rs',
 'crates/airk-retrieval/src/arbiter/runtime/qualification/CONTRACT.md',
 'crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration/compact_graph.rs',
 'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/attention_diagnostic.rs',
]
# Existing source identities allowed to change ONLY through reviewed actual
# new build seals. Math/compiler/context policy paths remain exactly old pins.
CHANGED_OLD_SOURCES={
 'crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration.rs',
 'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2.rs',
 'crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md',
}

def need(ok,message):
 if not ok:raise ValueError(message)
def pin(path):
 path=Path(path).absolute();need(path.is_file() and not path.is_symlink(),'Regular original file required: '+str(path))
 before=path.stat()
 with path.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
 after=path.stat();need((before.st_ino,before.st_size,before.st_mtime_ns)==(after.st_ino,after.st_size,after.st_mtime_ns),'Input changed during hash')
 return {'path':str(path),'sha256':h}
def checked(path,expected):
 value=pin(path);need(value['sha256']==expected,'Externally pinned bytes differ: '+str(path));return value

def derive(original,graph,build,sources,output,commit):
 need(original['schema']=='airk.private.context-metadata-regeneration/1' and 'compact_graph' not in original,'Original native /1 input required')
 result=copy.deepcopy(original)
 result.update(schema='airk.private.context-metadata-regeneration/2',compact_graph=copy.deepcopy(graph),native_build_receipt=copy.deepcopy(build),source_files=copy.deepcopy(sources),output=str(output),source_commit=commit)
 changed={'schema','compact_graph','native_build_receipt','source_files','output','source_commit'}
 need({k:v for k,v in original.items() if k not in changed}=={k:v for k,v in result.items() if k not in changed},'Opaque original metadata context changed')
 return result

def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--build-receipt');p.add_argument('--build-receipt-sha256');p.add_argument('--image-inventory');p.add_argument('--image-inventory-sha256');p.add_argument('--stage',action='store_true')
 a=p.parse_args()
 if not a.stage:
  print(json.dumps({'stage_performed':False,'required_actual_inputs':['--build-receipt','--build-receipt-sha256','--image-inventory','--image-inventory-sha256'],'native_emission':False,'model_execution':False}));return
 need(all([a.build_receipt,a.build_receipt_sha256,a.image_inventory,a.image_inventory_sha256]),'Actual new build/image pins required; no defaults/oldimage fallback')
 old=checked(ORIGINAL,ORIGINAL_SHA);original=json.loads(ORIGINAL.read_bytes())
 receipt_pin=checked(a.build_receipt,a.build_receipt_sha256);receipt=json.loads(Path(a.build_receipt).read_bytes())
 inventory_pin=checked(a.image_inventory,a.image_inventory_sha256);inventory=json.loads(Path(a.image_inventory).read_bytes())
 need(receipt['schema']=='airk-readiness-command/1' and receipt['exit_code']==0 and receipt['inputs_unchanged'] is True
      and receipt['changed_groups']==[] and receipt['runner_unchanged'] is True and receipt['runtime_unchanged'] is True and receipt['command_inputs_unchanged'] is True,'Actual successful unchanged native build receipt required')
 group=receipt['groups']['native'];need(inventory['schema']=='airk.preserved-native-test-images/1' and inventory['native_group']==group['sha256'] and inventory['actual_build_receipt']==receipt_pin,'Current images must bind this exact actual native build')
 commit=inventory['source_commit'];need(len(commit)==40 and all(c in '0123456789abcdef' for c in commit),'Actual source commit required')
 images={v['binary_id']:v for v in inventory['images']}
 for key in ['airk-retrieval','airk-inference']:checked(images[key]['path'],images[key]['sha256'])
 for key in ['source_catalog','acquisition_receipt','asset_census']:checked(original[key]['path'],original[key]['sha256'])
 need(Path(original['asset_root']).is_dir(),'Unchanged original acquired assetRoot unavailable')
 graph={key:checked(path,h) for key,(path,h) in FIXED.items()}
 files=[];seen=set()
 for row in original['source_files']:
  path=Path(row['path']);relative=path.relative_to(REPO).as_posix()
  if relative not in CHANGED_OLD_SOURCES:checked(path,row['sha256'])
  current=pin(path);need(group['files'].get(relative)==current['sha256'],'Source not sealed by actual new build: '+relative)
  files.append(current);seen.add(relative)
 for relative in ADDED_SOURCES:
  if relative in seen:continue
  current=pin(REPO/relative);need(group['files'].get(relative)==current['sha256'],'New native owner source missing from actual build: '+relative)
  files.append(current);seen.add(relative)
 need(len(files)<=128,'Native source bound exceeded')
 request=ROOT/'compact-gqa-native-metadata-r1/metadata-request.private.json';output=request.parent/'metadata'
 need(not request.parent.exists() and not output.exists(),'Fresh protected request/output required')
 result=derive(original,graph,receipt_pin,files,output,commit)
 # No native output directory is created here. The native owner owns metadata.
 request.parent.mkdir()
 with request.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 staged={'request':pin(request),'original_request':old,'image_inventory':inventory_pin,'retrieval_image':images['airk-retrieval'],'inference_image':images['airk-inference'],
  'native_output':str(output),'source_files':len(files),'NN_invocations':0,'native_emission':False,
  'command':[images['airk-retrieval']['path'],'--exact','arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets','--ignored','--nocapture'],
  'environment':{'AIRK_CONTEXT_METADATA_REGEN_INPUT':str(request),'AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256':pin(request)['sha256']}}
 with (request.parent/'staging.json').open('x',encoding='utf-8') as f:json.dump(staged,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 print(json.dumps(staged))
if __name__=='__main__':main()