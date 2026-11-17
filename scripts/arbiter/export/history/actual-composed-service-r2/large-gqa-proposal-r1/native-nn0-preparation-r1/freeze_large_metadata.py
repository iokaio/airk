# SPDX-License-Identifier: Apache-2.0
"""Freeze existing Rust-owned opaque metadata intake; never emit Profiles or invoke native."""
from pathlib import Path
import argparse,copy,hashlib,json,subprocess,os
R=Path(r'D:\code\Github\airk')
B=R/'artifacts/advanced-split/models/actual-composed-service-r2'
O=B/'large-gqa-proposal-r1'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
def digest(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def checked(p):
    p=Path(p);assert p.is_absolute() and p.is_file() and not p.is_symlink()
    for parent in p.parents:assert not parent.is_symlink() and not (getattr(parent.lstat(),'st_file_attributes',0)&0x400)
    assert p.stat().st_nlink==1
    return p
def pinned(p):
    checked(p['path']);assert digest(p['path'])==p['sha256'];return Path(p['path'])
def like(template,path):
    value=copy.deepcopy(template);assert set(value)=={'path','sha256'}
    value['path']=str(checked(path));value['sha256']=digest(path);return value
def write(p,value):
    with p.open('x',encoding='utf-8',newline='\n') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
    a=argparse.ArgumentParser(description=__doc__)
    for name in ('images','images-sha256','native-group','source-commit'):a.add_argument('--'+name,required=True)
    x=a.parse_args()
    sid=subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True);assert SID in sid
    packet_path=checked(Path(x.images));assert digest(packet_path)==x.images_sha256
    packet=json.loads(packet_path.read_bytes());assert packet['schema']=='airk.preserved-native-test-images/1' and packet['native_group']==x.native_group
    receipt_pin=packet['actual_build_receipt'];receipt=json.loads(pinned(receipt_pin).read_bytes())
    assert receipt['passed'] is True and receipt['status']=='completed' and receipt['exit_code']==receipt['runner_exit_code']==0
    assert all(receipt[k] is True for k in ('inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged'))
    assert receipt['changed_groups']==[] and receipt['groups']['native']['sha256']==x.native_group
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==x.source_commit
    images={}
    for role in ('airk-retrieval','airk-inference'):
        rows=[v for v in packet['images'] if v['binary_id']==role];assert len(rows)==1
        image=rows[0];assert image['original_file_owner_sid']==image['preserved_file_owner_sid']==SID
        pinned(image);images[role]=image
    template_path=B/'compact-gqa-native-metadata-r1/metadata-request.private.json'
    assert digest(template_path)=='044c1788a4ff63839835b3f969817d56d1c5f22babe7db3140a361c96c960f85'
    request=json.loads(template_path.read_bytes());assert request['schema']=='airk.private.context-metadata-regeneration/2'
    large=request.pop('compact_graph');assert 'large_graph' not in request
    graph=O/'large-gqa-graph-r1'
    for field,name,expected in [('graph','model.onnx','e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca'),('shape_proof','model.proof.json','6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b'),('owned_creation','owned-execution.json','2dcad61358578701be588d20b56787c0e169835d5329a5c760bdd662db2f857a')]:
        large[field]=like(large[field],graph/name);assert large[field]['sha256']==expected
    baseline=B/'final-plan-current-policy-preparation-20261116T085632Z/metadata-watchdog-b2-r4/metadata/arbiter-context-qwen3-4b/resource-run-input.private.json'
    large['baseline_input']=like(large['baseline_input'],baseline)
    request['large_graph']=large
    request['native_build_receipt']=like(request['native_build_receipt'],pinned(receipt_pin))
    request['source_commit']=x.source_commit
    sources={v['path']:v for v in request['source_files']}
    for rel in ['crates/airk-retrieval/src/arbiter/runtime.rs','crates/airk-retrieval/src/arbiter/runtime/qualification/CONTRACT.md','crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration/compact_graph.rs','crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/attention_diagnostic.rs','crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md']:
        sources[str(R/rel)]=like(request['source_files'][0],R/rel)
    source_files=[]
    for path,old in sources.items():
        value=like(old,Path(path));relative=Path(path).relative_to(R).as_posix()
        assert receipt['groups']['native']['files'][relative]==value['sha256'],relative
        source_files.append(value)
    request['source_files']=source_files;assert len(source_files)<=128
    for field in ('source_catalog','acquisition_receipt','asset_census'):pinned(request[field])
    asset_root=Path(request['asset_root']);assert asset_root.is_dir() and not asset_root.is_symlink()
    expected_root=B/'final-plan-acquired-assets-20261116T071657Z';assert asset_root.resolve(strict=True)==expected_root.resolve(strict=True)
    fresh=O/'large-native-metadata-r1';assert not fresh.exists();fresh.mkdir()
    request['output']=str(fresh/'metadata')
    request_path=fresh/'metadata-request.private.json';write(request_path,request)
    request_pin=like(request['source_catalog'],request_path)
    native_input=fresh/'metadata/arbiter-context-qwen3-4b/resource-run-input.private.json'
    staging={'request':request_pin,'image_inventory':{'path':str(packet_path),'sha256':x.images_sha256},'actual_build_receipt':receipt_pin,'native_group':x.native_group,'source_commit':x.source_commit,'source_dirty':True,'retrieval_image':images['airk-retrieval'],'inference_image':images['airk-inference'],'metadata_command':[images['airk-retrieval']['path'],'--exact','arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets','--ignored','--nocapture'],'metadata_environment':{'AIRK_CONTEXT_METADATA_REGEN_INPUT':str(request_path),'AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256':request_pin['sha256']},'template_command':[images['airk-inference']['path'],'--exact','inference_engine::pool::arbiter::qualification::software_v2::emit_software_v2_native_numeric_template','--ignored','--nocapture'],'template_environment_after_actual_metadata':{'AIRK_SOFTWARE_V2_NATIVE_INPUT':str(native_input),'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':'REQUIRES_ACTUAL_NATIVE_OUTPUT_SHA256_NO_PLACEHOLDER_EXECUTION','AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(fresh/'blank-template-large.json')},'source_files':len(source_files),'native_emission':False,'scope':'Freeze only. Root launches each through retained same-human bounded owner; no direct unowned launch. Template input hash must be filled from actual emitted native bytes. Python constructs no Profile/compiler/calibration/ResourceRunInput.'}
    write(fresh/'staging.json',staging)
    print(json.dumps({'staging':str(fresh/'staging.json'),'request':request_pin,'native_emission':False}))
if __name__=='__main__':main()
