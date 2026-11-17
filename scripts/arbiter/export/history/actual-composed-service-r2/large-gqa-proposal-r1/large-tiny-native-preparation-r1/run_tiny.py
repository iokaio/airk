# SPDX-License-Identifier: Apache-2.0
"""One original120s/8GiB synthetic GQA native control; use17 then8192, stop first failure."""
import argparse,json,math,os,re,sys,time
from pathlib import Path
BASE=Path(__file__).resolve().parent
OWNER=Path(r'D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\compact-native-diagnostic-owner-r1')
sys.path.insert(0,str(OWNER))
from runner import SID,need,pin,load,digest,write,unaliased,source_modules,final_observation
from security import observe
SELECTORS={17:'inference_engine::pool::arbiter::qualification::software_v2::attention_diagnostic::software_v2_synthetic_large_gqa_allocation_t17',
8192:'inference_engine::pool::arbiter::qualification::software_v2::attention_diagnostic::software_v2_synthetic_large_gqa_allocation_t8192'}
def main():
    started=time.monotonic();deadline=started+120
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--preparation-sha256',required=True);parser.add_argument('--image-inventory',required=True)
    parser.add_argument('--image-inventory-sha256',required=True);parser.add_argument('--tokens',type=int,choices=[17,8192],required=True)
    parser.add_argument('--execute-reviewed',action='store_true');a=parser.parse_args()
    preparation=pin({'path':str(BASE/'preparation.json'),'sha256':a.preparation_sha256});card=json.loads(preparation.read_bytes())
    pin(card['script']);pin(card['owner_preparation']);owner_card=source_modules()
    for value in card['native_sources'].values():pin(value)
    fixture=pin(card['fixture']);need(fixture.stat().st_size==617,'Exact original synthetic fixture required')
    packet_pin={'path':a.image_inventory,'sha256':a.image_inventory_sha256};packet=json.loads(pin(packet_pin).read_bytes())
    need(packet['schema']=='airk.preserved-native-test-images/1','Actual preserved image inventory required')
    images=[v for v in packet['images'] if v['binary_id']=='airk-inference'];need(len(images)==1,'Exactly one genuine Inference image required');image=images[0];pin(image)
    need(image['preserved_file_owner_sid']==image['original_file_owner_sid']==SID,'Wrong actual image owner')
    build_pin=packet['actual_build_receipt'];build=json.loads(pin(build_pin).read_bytes())
    need(build['passed'] is True and build['inputs_unchanged'] is True and packet['native_group']==build['groups']['native']['sha256'],'Actual build failed/changed')
    for name,value in card['native_sources'].items():need(build['groups']['native']['files'].get(name)==value['sha256'],'Actual image lacks frozen tiny owner/source: '+name)
    seals={v['path']:v['sha256'] for v in owner_card['source_pins'].values()}
    for v in [card['script'],card['owner_preparation'],card['fixture'],packet_pin,build_pin,image,*card['native_sources'].values()]:seals[v['path']]=v['sha256']
    seals[str(preparation)]=a.preparation_sha256
    runtime={'runtime_manifest':owner_card['runtime_manifest'],'input_seals':seals}
    sys.path.insert(0,owner_card['reference_root'])
    worker=load(owner_card['source_pins']['sid_reader']['path'],'tiny_native_sid')
    parent=load(owner_card['source_pins']['reference_parent']['path'],'tiny_native_parent')
    owner=load(owner_card['source_pins']['reference_owner']['path'],'tiny_native_owned')
    need(worker.windows_sid()==SID,'Wrong actual human SID');worker.verify_loaded_runtime(runtime)
    output=BASE.parent/f'large-tiny-t{a.tokens}-r1'
    need(not output.exists() and unaliased(output.parent)==BASE.parent and observe(output.parent)==card['approved_output_parent_security'],'Fresh approved human output required; no retries')
    if not a.execute_reviewed:
        print(json.dumps({'preflight_passed':True,'tokens':a.tokens,'native_image_sha256':image['sha256'],'native_executed':False}));return 0
    output.mkdir();(output/'temp').mkdir();raw=output/'native-result.json';stdout=output/'stdout.log';stdin=output/'stdin.bin'
    with stdin.open('xb'):pass
    argv=[str(pin(image)),'--ignored','--exact',SELECTORS[a.tokens],'--nocapture']
    env=dict(os.environ);env.update({'TEMP':str(output/'temp'),'TMP':str(output/'temp'),'TMPDIR':str(output/'temp'),
        'ORT_GQA_DISABLE_FLASH_ATTENTION':'0','AIRK_GQA_TINY_GRAPH':str(fixture),'AIRK_GQA_TINY_GRAPH_SHA256':card['fixture']['sha256'],'AIRK_GQA_TINY_OUTPUT':str(raw)})
    receipt={'schema':'airk.private.large-tiny-native-owned/1','complete':False,'failure':None,'post_input_drift':[],
        'tokens':a.tokens,'preparation_sha256':a.preparation_sha256,'image_inventory':packet_pin,'command':argv,'limits':{'original_seconds':120,'containment_bytes':8<<30,'log_bytes':64<<20},
        'fixture':card['fixture'],'model_weights_loaded':False,'human_or_release_qualification':False,'pins':{}}
    write(output/'started.json',receipt)
    def before(api,job,info,child_deadline):
        return parent.suspended_authentication(api,info,deadline,child_deadline,SID,image,owner.image_proof,sid_reader=worker.windows_sid)
    try:
        remaining=deadline-time.monotonic();need(remaining>0,'Original120s expired before launch')
        with stdin.open('rb') as inp,stdout.open('xb') as log:
            actual=owner.run_windows_owned_training(argv,output,log,remaining,env,inp,8<<30,before_resume=before,absolute_deadline=deadline)
        receipt['owned']=actual;need(parent.settled(actual),'Actual tiny process/Job/handles failed natural single-child completion')
        need(re.search(r'(?m)^running 1 test\s*$',stdout.read_text(encoding='utf-8',errors='replace')) is not None,'Exactly one original selector required')
        body=json.loads(raw.read_bytes());receipt['native_body']=body
        need(body['tokens']==a.tokens and body['graph_outputs']==['context'] and body['model_weights_loaded'] is False,'Actual tiny ABI/population differs')
        error=body['maximum_error'];need(type(error) in (int,float) and math.isfinite(error) and 0<=error<=.0001,'Actual synthetic value gate refused')
        for name,path in [('body',raw),('profiling',unaliased(body['profiling_path'])),('optimized',unaliased(body['optimized_graph_path']))]:
            need(path.parent.samefile(output) and path.is_file() and 0<path.stat().st_size<=64<<20,'Missing/outside/oversized actual tiny output')
            receipt['pins'][name]={'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)}
        receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        if hasattr(error,'owned_cleanup'):receipt['owned_cleanup']=error.owned_cleanup
        for name,path in [('body',raw),*[(f'partial-{v.name}',v) for v in output.glob('synthetic-large-gqa-*')]]:
            if path.is_file() and path.stat().st_size<=64<<20:receipt['pins'][name]={'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)}
    finally:
        for path,sha in seals.items():
            try:
                if digest(path)!=sha:receipt['post_input_drift'].append(path)
            except OSError:receipt['post_input_drift'].append(path)
        try:worker.verify_loaded_runtime(runtime)
        except BaseException as error:receipt['post_input_drift'].append(str(error))
        final_observation(receipt,started,deadline,stdout)
        # final_observation uses the supplied original cutoff; change only its
        # generic timeout message to identify this120s synthetic phase.
        if receipt.get('failure',{} ) is not None and receipt['failure'].get('type')=='OriginalPhaseExpired':receipt['failure']['message']='Final observations exceeded original120s tiny cutoff'
        write(output/'execution.json',receipt)
    print(json.dumps({'complete':receipt['complete'],'failure':receipt['failure'],'output':str(output)}));return 0 if receipt['complete'] else 1
if __name__=='__main__':sys.exit(main())
