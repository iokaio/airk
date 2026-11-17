# SPDX-License-Identifier: Apache-2.0
"""Bind genuine future Service build data; never creates build/test evidence or runs AIRK."""
import argparse,copy,hashlib,importlib.util,json
from pathlib import Path
BASE=Path(__file__).resolve().parent

def need(ok,message):
    if not ok:raise ValueError(message)
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def pinned(path,expected):
    path=Path(path);need(path.is_absolute() and path.is_file() and not path.is_symlink(),'Ordinary absolute build artifact required')
    need(not path.lstat().st_file_attributes&0x400 and path.stat().st_nlink==1,'Artifact alias/reparse refused')
    need(sha(path)==expected,'Actual artifact SHA differs: '+str(path));return path

def validate_build(inventory,receipt,value):
    need(inventory['schema']=='airk.preserved-native-test-images/1','Existing actual test-image inventory schema required')
    need(receipt['schema']=='airk-readiness-command/1' and receipt.get('passed') is True
        and receipt.get('exit_code')==0 and receipt.get('inputs_unchanged') is True
        and '--no-run' in receipt['command'],'Genuine unchanged passing Cargo no-run receipt required')
    native=receipt['groups']['native']
    need(native['sha256']==inventory['native_group']==value['expected_corrected_native_group'],
        'Actual future whole-source group differs from reviewed corrected source')
    for source,expected in value['required_corrected_service_sources'].items():
        need(native['files'].get(source)==expected,'Build did not consume exact corrected Service source: '+source)

def unique_image(inventory,name):
    rows=[row for row in inventory['images'] if row['binary_id']==name]
    need(len(rows)==1,'Exact actual component artifact required: '+name)
    return rows[0]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--template-sha256',required=True);parser.add_argument('--inventory',required=True)
    parser.add_argument('--inventory-sha256',required=True)
    args=parser.parse_args();template=BASE/'launch-card.template.json'
    need(sha(template)==args.template_sha256,'Reviewed R4 template changed')
    value=json.loads(template.read_bytes());need(value['pending_actual_service_binding'] is True,'Only unbound reviewed template accepted')
    inventory_path=pinned(args.inventory,args.inventory_sha256);inventory=json.loads(inventory_path.read_bytes())
    build_pin=inventory['actual_build_receipt'];build_path=pinned(build_pin['path'],build_pin['sha256']);receipt=json.loads(build_path.read_bytes())
    validate_build(inventory,receipt,value)
    source=value['inference_component_anchor'];old_path=pinned(source['inventory']['path'],source['inventory']['sha256']);old=json.loads(old_path.read_bytes())
    need(old['native_group']==source['native_group'],'Original Inference evidence group changed')
    old_inference=unique_image(old,'airk-inference');new_inference=unique_image(inventory,'airk-inference');service=unique_image(inventory,'airk-service')
    need(old_inference['sha256']==new_inference['sha256']==source['image_sha256'],
        'Inference bytes changed; old5d37 evidence cannot transfer without separate reviewed work')
    # Actual image hashes and human ownership, not a source-group label, establish equality.
    spec=importlib.util.spec_from_file_location('existing_staging_guard',value['staging_guard']);guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
    for image in [old_inference,new_inference,service]:
        path=pinned(image['path'],image['sha256']);need(path.stat().st_size==image['bytes'],'Actual image size differs');guard.security(path)
    bridge={'scope':'exact unchanged Inference executable bytes only; no whole-source identity or qualification relabel',
        'original_inventory':source['inventory'],'original_native_group':old['native_group'],
        'actual_new_inventory':{'path':str(inventory_path),'sha256':args.inventory_sha256},
        'actual_new_native_group':inventory['native_group'],'original_inference':old_inference,'actual_new_inference':new_inference,
        'same_inference_image_sha256':source['image_sha256'],'byte_identity_equal':True,
        'profile_source_manifests':'Original198f/5d37 model/profile/source evidence stays under its own genuine identities; no current-profile promotion'}
    bridge_path=BASE/'inference-component-byte-bridge.json'
    with bridge_path.open('x',encoding='utf-8') as stream:json.dump(bridge,stream,indent=2)
    value['service_image']={key:service[key] for key in ['path','sha256','bytes']};value['native_group']=inventory['native_group'];value['pending_actual_service_binding']=False
    value['actual_service_build']={'inventory':{'path':str(inventory_path),'sha256':args.inventory_sha256},'build_receipt':build_pin,
        'scope':'Actual corrected Service test image from genuine passing no-run build; runtime acceptance remains pending'}
    value['inference_component_bridge']={'path':str(bridge_path),'sha256':sha(bridge_path)}
    for path,expected in [(inventory_path,args.inventory_sha256),(build_path,build_pin['sha256']),(old_path,source['inventory']['sha256']),
        (Path(old_inference['path']),old_inference['sha256']),(Path(new_inference['path']),new_inference['sha256']),
        (Path(service['path']),service['sha256']),(bridge_path,sha(bridge_path)),(template,args.template_sha256),(Path(__file__).resolve(),sha(__file__))]:
        value['pins'][str(path)]=expected
    destination=BASE/'launch-card.json'
    with destination.open('x',encoding='utf-8') as stream:json.dump(value,stream,indent=2)
    print(json.dumps({'card':str(destination),'card_sha256':sha(destination),'bridge':str(bridge_path),'bridge_sha256':sha(bridge_path),
        'actual_service_sha256':service['sha256'],'actual_source_group':inventory['native_group'],'inference_group_not_relabeled':old['native_group']}))

if __name__=='__main__':main()