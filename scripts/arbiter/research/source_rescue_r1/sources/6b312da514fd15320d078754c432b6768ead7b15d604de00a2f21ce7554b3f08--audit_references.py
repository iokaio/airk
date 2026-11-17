# SPDX-License-Identifier: Apache-2.0
# Private reference data audit. No model/native imports; never reads weights.
import hashlib,json,math,pathlib,struct,datetime
ROOT=pathlib.Path(__file__).resolve().parents[5]
# Paths deliberately limited to the exact Root-approved three native templates
# and retained independent executions, rather than discovery/search.
P=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2'
W=ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/runs-native-r1/revision-r2'
CURRENT=P/'gpu-current-policy-original3-metadata-r1'
MODELS=[('focus','arbiter-focus-qwen3-reranker-06b'),('compact','arbiter-context-qwen3-17b'),('large','arbiter-context-qwen3-4b')]
def load(p):return json.loads(pathlib.Path(p).read_bytes())
def digest(raw):return hashlib.sha256(raw).hexdigest()
def canonical(v):return json.dumps(v,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False).encode('ascii')
def equal(a,b):return canonical(a)==canonical(b)
def pin(p):
 p=pathlib.Path(p);raw=p.read_bytes();return {'path':str(p),'bytes':len(raw),'sha256':digest(raw)}
def verified(p):
 observed=pin(p['path']);assert observed['sha256']==p['sha256'];assert 'bytes' not in p or observed['bytes']==p['bytes'];return observed

def audit():
 result=[];all_pins={};missing=[]
 def keep(p):
  q=pin(p);all_pins[q['path']]=q;return q
 def verify(p):
  q=verified(p);all_pins[q['path']]=q;return q
 native_report=keep(CURRENT/'metadata-original3/metadata-regeneration-report.private.json');report=load(native_report['path'])
 assert report['complete'] is True and report['NN_invocations']==0 and report['release_qualified'] is False
 keep(CURRENT/'native-template-commands.json')
 for short,model in MODELS:
  folder=CURRENT/'metadata-original3'/model;template_pin=keep(CURRENT/'numeric-templates'/(model+'.json'));current=load(template_pin['path'])
  old_pin=keep(W/('assembled-'+short)/'numeric-fixture.json');old=load(old_pin['path']);producer_pin=verify(old['reference_producer']);producer=load(producer_pin['path'])
  request_pin=keep(folder/'resource-run-input.private.json');request=load(request_pin['path']);profile_pin=verify(request['profile']);profile=load(profile_pin['path'])
  original_template=verify(producer['native_template']);assert template_pin['sha256']==original_template['sha256']
  assert equal(old['rows'],producer['rows']) and equal(old['original_reference_assets'],producer['original_reference_assets'])
  for name in ['schema','model','compiler','resource_policy']:assert equal(current[name],old[name])
  assert current['model']==model and current['compiler']==producer['compiler']==profile['compiler'] and current['resource_policy']==producer['resource_policy_digest']==request['policy_digest']
  assets=[a for a in profile['assets'] if a['role'] in ('model','external_weights','tokenizer')]
  assert equal(assets,producer['candidate_assets']);assert equal(producer['original_reference_assets'],old['original_reference_assets'])
  assert [x['name'] for x in current['rows']]==[x['name'] for x in old['rows']]
  rows=[]
  for now,prior in zip(current['rows'],old['rows']):
   assert equal(now['input'],prior['input']) and now['reference_same_artifact'] is None and now['reference_original_fp32'] is None
   fields={key:{'equal':equal(value,prior['input'][key]),'value_sha256':digest(canonical(value)),'elements':len(value) if isinstance(value,list) else None} for key,value in now['input'].items()}
   refs={}
   for kind in ['reference_same_artifact','reference_original_fp32']:
    values=prior[kind];assert isinstance(values,list) and values and all(type(x) in (int,float) and math.isfinite(x) for x in values)
    assert all(float(struct.unpack('<f',struct.pack('<f',v))[0])==v for v in values)
    refs[kind]={'values':values,'elements':len(values),'json_sha256':digest(canonical(values)),'f32_le_sha256':digest(b''.join(struct.pack('<f',v) for v in values))}
   rows.append({'name':now['name'],'all_input_fields_equal':True,'fields':fields,'reference_arrays':refs})
  executions=[]
  for execution_pin in producer['reference_executions']:
   ep=verify(execution_pin);execution=load(ep['path']);raw_pin=verify(execution['raw_output']);raw=load(raw_pin['path']);kind=execution['reference_kind'];column='reference_'+kind
   assert kind in ('same_artifact','original_fp32') and execution['complete'] is True and execution['failure'] is None and execution['post_input_drift'] in ({},[]) and execution['native_airk_nn_invocations']==0
   assert execution['whole_phase_deadline_not_refreshed'] is True and execution['whole_phase_expired'] is False and execution['execution_user_sid'].endswith('-1002')
   owned=execution['owned'];close=owned['handle_close_proof'];assert owned['exit_code']==0 and owned['owned_descendants_settled'] is True and owned['owned_job_forced_termination'] is False and owned['owned_job_final_active_processes']==0 and owned['owned_job_final_member_pids']==[] and close['settled'] is True and close['forced'] is False and close['final_active']==0 and close['final_member_pids']==[] and close['members_before']==[]
   assert equal(raw['rows'],execution['rows']) and raw['complete'] is True and execution['torch_forward_invocations']==len(old['rows'])
   assert equal(raw['candidate_assets'],assets) and equal(raw['original_reference_assets'],old['original_reference_assets'])
   source_assets=assets if kind=='same_artifact' else old['original_reference_assets'];assert equal(execution['source_assets'],source_assets)
   for asset in source_assets:
    path=request['files'][asset['name']] if kind=='same_artifact' else producer['original_reference_files'][asset['name']]['path']
    assert execution['input_seals'][path]==asset['sha256'] and pathlib.Path(path).stat().st_size==asset['bytes']
   for observed,expected in zip(execution['rows'],old['rows']):
    assert observed['name']==expected['name'] and equal(observed['input'],expected['input']) and equal(observed['scores'],expected[column])
   assert len(execution['rows'])==len(old['rows'])
   source_pins=[]
   for path,expected_sha in execution['input_seals'].items():
    if path.endswith('.py') and 'site-packages' not in path and 'Lib' not in pathlib.PureWindowsPath(path).parts:
     source_pins.append(verify({'path':path,'sha256':expected_sha}))
   # Original publisher source configs/indexes/tokenizer are metadata/text;
   # checkpoints remain pinned by retained native/execution seals, never read.
   for name,source in producer['original_source_files'].items():
    assert execution['input_seals'][source['path']]==source['sha256']
    if not name.endswith('.safetensors'):verify(source)
   verify(execution['stdout']);verify(owned['suspended_assignment']['image'])
   executions.append({'kind':kind,'execution':ep,'raw_output':raw_pin,'all_rows_inputs_scores_equal':True,'source_assets_and_paths_match_current':True,'custom_source_files_rehashed':source_pins,'retained_original_settlement_inspected':True,'current_process_or_resource_verdict_transferred':False})
  assembly=keep(W/('assembled-'+short)/'assembly.json');assembled=load(assembly['path']);verify(assembled['producer']);verify(assembled['fixture']);assert assembled['qualification_granted'] is False and assembled['model_executed_by_assembler'] is False
  assembly_execution=keep(W/(short+'-assembly-execution.json'));assert load(assembly_execution['path'])['exit_code']==0
  result.append({'model':model,'current_template':template_pin,'current_input':request_pin,'current_profile':profile_pin,'retained_numeric_fixture':old_pin,'retained_reference_producer':producer_pin,'original_native_template':original_template,'byte_identical_template':True,'input_digest_equal_no_normalization':True,'candidate_assets_exact_current':assets,'rows':rows,'reference_executions':executions,'old_qualification_transferred':False,'binding_allowed':True})
 return {'schema':'airk.private.original3-reference-reuse-audit/1','created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'human':'TYAI\\tdotn','sid':'S-1-5-21-3678370870-3858642417-826968105-1002','verdict':'All18 native rows and both independent reference kinds match; original reference data may be bound without changing producer/fixture bytes','native_report':native_report,'native_source_group':report['native_source_group'],'native_binary_sha256':report['native_binary_sha256'],'models':result,'pinned_audit_inputs':list(all_pins.values()),'missing_rows':missing,'input_digest_normalization':None,'reference_rows':sum(len(x['rows']) for x in result),'new_model_native_Torch_GPU_execution':0,'new_weight_payload_reads':0,'qualification':False,'limits':['Fresh verification of large checkpoint/external-weight and full runtime-library byte seals is delegated to the existing native consumer; this bounded audit compares exact retained native/execution hash descriptors and file lengths without reading weight payloads.','References retain original producer/fixture/execution/source identities. No old numeric pass, task, CAL, resource, placement, GPU or latency verdict is transferred.','Current native metadata has fresh profile/CAL/task identities; reference array reuse does not approve those identities.','UTF8 JSON read_bytes decoding is required; no Windows codepage projection, trimming, semantic float tolerance or digest rewrite is permitted.']}
if __name__=='__main__':
 out=pathlib.Path(__file__).with_name('audit.json');assert not out.exists();out.write_text(json.dumps(audit(),indent=2)+'\n',encoding='utf-8');print(str(out));print(digest(out.read_bytes()))
