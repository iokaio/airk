# SPDX-License-Identifier: Apache-2.0
"""Stage exact future dense tiny cards only from actual immutable image and fixture pins; no native launch."""
import argparse,hashlib,json,sys
from pathlib import Path
BASE=Path(__file__).resolve().parent
OWNER=Path(r'D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\compact-native-diagnostic-owner-r1')
sys.path.insert(0,str(OWNER))
from runner import SID,pin,source_modules,load,need,write,digest
from security import observe
OLD_FIXTURE={'path':str(BASE.parents[1]/'large-synthetic-gqa-fixture-r1/synthetic-gqa.onnx'),'sha256':'5b4633f0343feb6dde8ae92a2e01b1bba9be2e008b8bfc5078ecdb4012bb984f'}
MARK=b'\x0a\x06causal\x18\x01\xa0\x01\x02'
def exact_dense_fixture(original,candidate):
    need(len(original)==len(candidate)==617 and original.count(MARK)==1,'Exact617B original causal1 fixture required')
    need(candidate==original.replace(MARK,MARK.replace(b'\x18\x01',b'\x18\x00')),'Only causal1 to causal0 fixture change admitted')
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['preparation-sha256','images','images-sha256','fixture','fixture-sha256']:p.add_argument('--'+name,required=True)
    p.add_argument('--stage',action='store_true');a=p.parse_args()
    prep=pin({'path':str(BASE/'preparation.json'),'sha256':a.preparation_sha256});frozen=json.loads(prep.read_bytes())
    for value in frozen['sources'].values():pin(value)
    owner=source_modules();worker=load(owner['source_pins']['sid_reader']['path'],'dense_factory_sid');need(worker.windows_sid()==SID,'Wrong actual human SID')
    expected=json.loads(pin(frozen['sources']['native_map']).read_bytes())
    fixture={'path':str(Path(a.fixture).resolve(strict=True)),'sha256':a.fixture_sha256};exact_dense_fixture(pin(OLD_FIXTURE).read_bytes(),pin(fixture).read_bytes())
    inventory={'path':str(Path(a.images).resolve(strict=True)),'sha256':a.images_sha256};packet=json.loads(pin(inventory).read_bytes())
    need(packet['schema']=='airk.preserved-native-test-images/1','Genuine immutable native image inventory required')
    buildpin=packet['actual_build_receipt'];build=json.loads(pin(buildpin).read_bytes())
    need(build.get('passed') is True and build.get('inputs_unchanged') is True and packet['native_group']==build['groups']['native']['sha256'],'Actual build failed/drifted')
    rows=[v for v in packet['images'] if v['binary_id']=='airk-inference'];need(len(rows)==1,'Exactly one actual Inference image required');image=rows[0];pin(image)
    need(image['preserved_file_owner_sid']==image['original_file_owner_sid']==SID,'Wrong image custody')
    for name,value in expected.items():
        pin(value);need(build['groups']['native']['files'].get(name)==value['sha256'],'Actual native build lacks exact reviewed dense source: '+name)
    destination=BASE/'cards';need(not destination.exists(),'Fresh cards required; no overwrite')
    card={'schema':'airk.private.large-dense-tiny-native-preparation/1','status':'actual inputs sealed; Root sole native dispatcher; no execution by staging',
        'script':frozen['sources']['runner'],'owner_preparation':frozen['owner_preparation'],'fixture':fixture,
        'native_sources':expected,'approved_output_parent_security':observe(BASE),'actual_image_inventory':inventory,
        'actual_build':buildpin,'actual_inference_image':image,'native_group':packet['native_group'],
        'limits':{'original_seconds_each':120,'containment_bytes':8<<30,'log_bytes':64<<20,'no_retries':True},
        'scope':'Dense causal0 synthetic17 then128 only; three mask controls per selector; no model, no8192/Flash qualification'}
    need(card['approved_output_parent_security']['owner_sid']==SID,'Wrong output owner')
    cardpath=destination/'preparation.json'
    if not a.stage:
        print(json.dumps({'actual_inputs_verified':True,'staged':False,'native_executed':False}));return
    destination.mkdir();write(cardpath,card);sha=digest(cardpath)
    prefix=[owner['parent_python']['path'],'-S','-B','-X','pycache_prefix='+str(OWNER/'never-created-runtime-cache'),str(BASE/'run_tiny.py')]
    commands={str(t):prefix+['--card',str(cardpath),'--preparation-sha256',sha,'--image-inventory',inventory['path'],'--image-inventory-sha256',inventory['sha256'],'--tokens',str(t),'--execute-reviewed'] for t in [17,128]}
    write(destination/'commands.json',{'card':{'path':str(cardpath),'sha256':sha},'commands':commands,'order':[17,128],'stop_on_first_failure':True,'native_executed':False})
    print(json.dumps({'card':{'path':str(cardpath),'sha256':sha},'commands':str(destination/'commands.json'),'native_executed':False}))
if __name__=='__main__':main()
