# SPDX-License-Identifier: Apache-2.0
"""Bind prepared private cards to actual new native NN0 inputs; zero process/model execution."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
from launch_native import BASE,pin,new_image

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['old-card','old-card-sha256','new-native-input','new-native-input-sha256',
        'new-blank-template','new-blank-template-sha256','new-image-packet','new-image-packet-sha256',
        'source-bridge','source-bridge-sha256','output']:
        p.add_argument('--'+name,required=True)
    a=p.parse_args();old=json.loads(pin(a.old_card,a.old_card_sha256).read_bytes())
    before=json.loads(pin(old['native_input']['path'],old['native_input']['sha256']).read_bytes())
    fresh_path=pin(a.new_native_input,a.new_native_input_sha256);fresh=json.loads(fresh_path.read_bytes())
    _,bridge=new_image(SimpleNamespace(image_packet=a.new_image_packet,image_packet_sha256=a.new_image_packet_sha256,
        source_bridge=a.source_bridge,source_bridge_sha256=a.source_bridge_sha256),old)
    manifest_path=pin(fresh['source_manifest']['path'],fresh['source_manifest']['sha256'])
    manifest=json.loads(manifest_path.read_bytes())
    if manifest['native_build_receipt']!=bridge['new_build_receipt']:
        raise ValueError('new Rust source manifest must bind actual patched image build receipt')
    if set(before)!=set(fresh) or any(before[k]!=fresh[k] for k in before if k not in ['source_manifest','profile','files']):
        raise ValueError('native request semantic/policy/numeric fixture changed')
    prior_profile=pin(before['profile']['path'],before['profile']['sha256'])
    current_profile=pin(fresh['profile']['path'],fresh['profile']['sha256'])
    if prior_profile.read_bytes()!=current_profile.read_bytes():
        raise ValueError('new profile bytes differ; old references cannot silently transfer')
    profile=json.loads(current_profile.read_bytes())
    if set(before['files'])!=set(fresh['files']):raise ValueError('native asset roster changed')
    for asset in profile['assets']:
        if asset['role'] in ['model','external_weights','tokenizer'] and before['files'][asset['name']]!=fresh['files'][asset['name']]:
            raise ValueError('exact neural/tokenizer paths sealed by original Torch executions changed')
    summary_pin=next({'path':path,'sha256':sha} for path,sha in old['input_seals'].items()
        if Path(path).name=='nn0-preparation-summary.json')
    summary=json.loads(pin(summary_pin['path'],summary_pin['sha256']).read_bytes())
    original=next(row for row in summary['models'] if row['model']==old['model'])
    original_template=pin(original['template']['path'],original['template']['sha256'])
    current_template=pin(a.new_blank_template,a.new_blank_template_sha256)
    if current_template.read_bytes()!=original_template.read_bytes():
        raise ValueError('new native blank tensor template differs; references cannot silently transfer')
    card=copy.deepcopy(old);card['native_input']={'path':str(fresh_path),'sha256':a.new_native_input_sha256}
    card['native_source_reconciliation']=bridge;card['status']='actual new NN0 inputs bound; Root review required before execution'
    for value in [fresh['source_manifest'],fresh['profile'],bridge['new_build_receipt'],bridge['new_image_packet'],bridge['combined_source_bridge'],
        {'path':str(fresh_path),'sha256':a.new_native_input_sha256},
        {'path':str(current_template),'sha256':a.new_blank_template_sha256}]:
        pin(value['path'],value['sha256']);card['input_seals'][value['path']]=value['sha256']
    for row in manifest['source_files']:
        pin(row['path'],row['sha256']);card['input_seals'][row['path']]=row['sha256']
    for asset in profile['assets']:
        path=Path(fresh['files'][asset['name']]).resolve(strict=True)
        if path.stat().st_size!=asset['bytes']:raise ValueError('actual new asset length differs')
        card['input_seals'][str(path)]=asset['sha256']
    card['new_native_blank_template']={'path':str(current_template),'sha256':a.new_blank_template_sha256,
        'byte_equal_to_original_native_template':True}
    output=Path(a.output).absolute()
    if output.exists() or not output.resolve().is_relative_to((BASE/'plans').resolve()):
        raise ValueError('fresh card must remain under protected private plans')
    output.parent.mkdir(parents=True,exist_ok=True)
    with output.open('x',encoding='utf-8') as stream:json.dump(card,stream,indent=2);stream.write('\n')
    print(json.dumps({'path':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),
        'native_or_model_execution':False,'original_producer_fixture_and_reference_executions_unchanged':True}))

if __name__=='__main__':main()
