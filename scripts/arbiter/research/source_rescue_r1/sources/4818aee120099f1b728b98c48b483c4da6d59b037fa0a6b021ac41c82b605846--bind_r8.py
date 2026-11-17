# SPDX-License-Identifier: Apache-2.0
"""Private R8 artifact binding; read/verify genuine Root outputs, never build/run AIRK."""
import argparse,copy,hashlib,importlib.util,json
from pathlib import Path
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[4]
GUARD=BASE.parent/'candidate-overlay-preparation-r1/candidate_overlay.py'

def need(value,message):
    if not value:raise ValueError(message)
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def read(path,limit=8*1024*1024):
    path=Path(path);need(path.stat().st_size<=limit,'Metadata exceeds bound')
    with path.open('rb') as stream:raw=stream.read(limit+1)
    need(len(raw)<=limit,'Metadata grew past bound')
    return load('r8_json_owner',ROOT/'scripts/check-service-clients.py').original_pair_json(raw)
def pin(path,expected,guard):
    path=Path(path);need(path.is_absolute() and path.is_file() and not path.is_symlink(),'Ordinary absolute artifact required')
    st=path.lstat();need(not getattr(st,'st_file_attributes',0)&0x400 and st.st_nlink==1,'Aliased/reparse artifact refused')
    need(guard.security(path)['owner_sid']==guard.HUMAN and sha(path)==expected,'Artifact ownership or bytes changed')
    return path

def validate_build_sources(receipt,template):
    need(receipt.get('schema')=='airk-readiness-command/1' and receipt.get('passed') is True
        and receipt.get('exit_code')==0 and receipt.get('inputs_unchanged') is True,
        'Genuine successful unchanged Root command receipt required')
    native=receipt['groups']['native']
    need(native['sha256']==template['expected_corrected_native_group'],'Command did not consume exact e306 native group')
    for name,expected in template['required_corrected_service_sources'].items():
        need(native['files'].get(name)==expected,'D06/fixture source missing or old: '+name)

def validate_pair_identity(metadata,build,template):
    owner=load('r8_c7_owner',ROOT/'scripts/split_receipts.py')
    owner.validate_pair_metadata(metadata);owner.validate_build_receipt(build)
    need(build['runtime']==metadata and build['source']==template['expected_pair_source'],
        'Pair metadata/C7 source differs from current e306 runtime')
    need((metadata['version'],metadata['os'],metadata['arch'])==('1.8.207','windows','x86_64'),
        'Exact current Windows pair required')

def validate_active_binding(value):
    need(value.get('pending_actual_pair_binding') is False and value.get('pending_actual_service_binding') is False,
        'Both genuine current Service and CLI bindings remain pending')
    p=value['r8_binding_proof'];path=Path(p['path']);need(sha(path)==p['sha256'],'R8 binding proof changed')
    proof=read(path)
    need(proof['expected_commit']==value['expected_source_commit'] and proof['native_group']==value['expected_corrected_native_group'],
        'R8 current source binding differs')
    need(value['service_image']==proof['service_image'] and value['client_pair']==proof['client_pair']
        and value['client_pair_build']==proof['client_pair_build'] and value['airkctl']==proof['airkctl'],
        'Old CLI/new Service or old Service/new CLI mixing refused')
    need(value['service_image']['sha256']!=value['historical_R7_binding']['service_image']['sha256']
        and sha(value['airkctl'])!=value['historical_R7_binding']['client_pair_build'].get('historical_airkctl_sha256','5a11763598890f9fecba1a9899d4e02704da4da56f20c9f248da680d3308f98f'),
        'Old unfixed image cannot be used for R8')
    need(sha(value['client_pair_build']['path'])==value['client_pair_build']['sha256'],'Current pair build bytes changed')
    metadata=read(Path(value['client_pair'])/'airk-runtime.json',4096)
    build=read(value['client_pair_build']['path'],16384)
    validate_pair_identity(metadata,build,value)
    need(sha(value['airkctl'])==metadata['airkctl_sha256'] and sha(value['service_image']['path'])==value['service_image']['sha256'],
        'Current active image differs from bound metadata')
    return proof

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('template-sha256','inventory','inventory-sha256','runtime','runtime-receipt','runtime-receipt-sha256'):parser.add_argument('--'+name,required=True)
    args=parser.parse_args();guard=load('r8_guard',GUARD);identity=guard.identity()
    need(guard.security(BASE)['owner_sid']==guard.HUMAN,'Private preparation owner differs')
    template=BASE/'launch-card.template.json';need(sha(template)==args.template_sha256,'Reviewed R8 template differs');value=read(template)
    need(value.get('pending_actual_pair_binding') is True and value.get('pending_actual_service_binding') is True,'Only pending template may bind')
    inventory_path=pin(args.inventory,args.inventory_sha256,guard);inventory=read(inventory_path)
    need(inventory.get('schema')=='airk.preserved-native-test-images/1' and inventory['native_group']==value['expected_corrected_native_group']
        and inventory['source_commit']==value['expected_source_commit'],'Genuine e306 preserved image inventory required')
    image_receipt_pin=inventory['actual_build_receipt'];image_receipt_path=pin(image_receipt_pin['path'],image_receipt_pin['sha256'],guard);image_receipt=read(image_receipt_path)
    validate_build_sources(image_receipt,value);need('--no-run' in image_receipt['command'],'Current images require actual no-run build receipt')
    service=[x for x in inventory['images'] if x['binary_id']=='airk-service'];need(len(service)==1,'One actual Service test image required');service=service[0]
    pin(service['path'],service['sha256'],guard);need(Path(service['path']).stat().st_size==service['bytes'],'Current Service size changed')
    runtime=Path(args.runtime).resolve(strict=True);need(runtime.is_dir() and guard.security(runtime)['owner_sid']==guard.HUMAN,'Current runtime owner differs')
    runtime_receipt_path=pin(args.runtime_receipt,args.runtime_receipt_sha256,guard);runtime_receipt=read(runtime_receipt_path)
    validate_build_sources(runtime_receipt,value)
    command=runtime_receipt['command'];need('scripts/build-service-runtime.py' in command and '--destination' in command,'Actual matched runtime builder receipt required')
    need(Path(command[command.index('--destination')+1]).resolve(strict=True)==runtime,'Runtime receipt names another destination')
    clients=load('r8_original_pair',ROOT/'scripts/check-service-clients.py');clients.original_split_pair_payload(runtime)
    metadata=read(runtime/'airk-runtime.json',4096);build=read(runtime/'airk-build.json',16384);validate_pair_identity(metadata,build,value)
    package=load('r8_package_owner',ROOT/'scripts/package-native.py')
    roster,executables=package.validate_runtime(runtime,metadata['version'],'x86_64-pc-windows-msvc',value['expected_pair_source'])
    for name,expected in roster.items():pin(runtime/name,expected,guard)
    need(set(executables)=={'airk.exe','airkctl.exe'},'Canonical complete pair image names differ')
    inference=[x for x in inventory['images'] if x['binary_id']=='airk-inference'];need(len(inference)==1,'Current inference component provenance missing');inference=inference[0];pin(inference['path'],inference['sha256'],guard)
    need(inference['sha256']==value['expected_current_inference_sha256'],
        'Fresh inference component differs from reviewed actual e306 inventory')
    value['service_image']={key:service[key] for key in ('path','sha256','bytes')};value['client_pair']=str(runtime);value['airkctl']=str(runtime/'airkctl.exe')
    value['client_pair_build']={'path':str(runtime/'airk-build.json'),'sha256':sha(runtime/'airk-build.json'),'bytes':(runtime/'airk-build.json').stat().st_size}
    value['actual_service_build']={'inventory':{'path':str(inventory_path),'sha256':sha(inventory_path)},'build_receipt':image_receipt_pin,'scope':'Actual e306 Service image; R8 runtime outcome pending'}
    value['actual_pair_build']={'path':str(runtime_receipt_path),'sha256':sha(runtime_receipt_path)}
    value['pending_actual_pair_binding']=False;value['pending_actual_service_binding']=False
    proof={'schema':'airk.private.r154-r8-active-artifacts/1','identity':identity,'expected_commit':value['expected_source_commit'],'native_group':value['expected_corrected_native_group'],
        'service_image':value['service_image'],'client_pair':value['client_pair'],'client_pair_build':value['client_pair_build'],'airkctl':value['airkctl'],
        'pair_C7_source':build['source'],'actual_service_build':value['actual_service_build'],'actual_pair_build':value['actual_pair_build'],'pair_payload_files':len(roster),
        'original_inference_anchor':value['inference_component_anchor'],'actual_inference_image':inference,'inference_byte_equal_to_original':inference['sha256']==value['inference_component_anchor']['image_sha256'],'old_scores_or_qualification_transferred':False,'qualification_scope':'Artifact identity only, never old whole-source or model qualification relabel','native_execution_by_binder':False}
    proof_path=BASE/'active-artifact-binding.json';need(not proof_path.exists() and not (BASE/'launch-card.json').exists(),'Fresh binding outputs required')
    proof_path.write_text(json.dumps(proof,indent=2)+'\n');value['r8_binding_proof']={'path':str(proof_path),'sha256':sha(proof_path)}
    for path in [inventory_path,image_receipt_path,runtime_receipt_path,proof_path,GUARD,Path(__file__).resolve(),template,*[runtime/n for n in roster],Path(service['path']),Path(inference['path'])]:value['pins'][str(path)]=sha(path)
    validate_active_binding(value)
    (BASE/'launch-card.json').write_text(json.dumps(value,indent=2)+'\n')
    print(json.dumps({'card':str(BASE/'launch-card.json'),'card_sha256':sha(BASE/'launch-card.json'),'binding_proof_sha256':sha(proof_path),'active_Service':value['service_image'],'active_airkctl_sha256':metadata['airkctl_sha256'],'native_group':value['native_group'],'pair_C7_source':build['source'],'no_native_execution':True}))

if __name__=='__main__':main()
