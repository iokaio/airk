# SPDX-License-Identifier: Apache-2.0
"""Pure seam controls only: no producer, archive, child process or filesystem fixtures."""
import copy,importlib.util,io,json,sys,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock,patch
HERE=Path(__file__).resolve().parents[2] if Path(__file__).parent.name=='tests' else Path(__file__).resolve().parent
def load(name):
 spec=importlib.util.spec_from_file_location(name.replace('-','_'),HERE/'scripts'/(name+'.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
runtime=load('cuda-static-runtime');env=load('cuda-runtime-environment');agg=load('cuda-static-aggregate');stage=load('cuda-provider-staging');cli=load('provision-cuda-static')
def proof(version=3):
 b={'schema':'airk.cuda-static-build-proof/'+str(version),'build_receipt':{'sha256':'c'*64},'qualification':False,'activation_pins_available':False}
 if version==3:b['new_aggregate']={'path':'C:/destination/aggregate/onnxruntime.lib','bytes':42,'sha256':'d'*64}
 return b
def aggregate(build):
 return {'schema':'airk.cuda-static-aggregate-proof/1','producer_proof_sha256':runtime.canonical(build),'output':{'path':'aggregate/onnxruntime.lib','bytes':42,'sha256':'d'*64},'receipt_sha256':'c'*64,'qualification':False}
class ConsumerControls(unittest.TestCase):
 def test_historical_projection_shapes_unchanged(self):
  for version in [1,2]:
   b=proof(version);a=aggregate(b);p=runtime.static_projection(b,a)
   self.assertEqual(p,{'build_receipt_sha256':'c'*64,'target':runtime.TARGET,'static_link_closure_sha256':runtime.canonical(a),'static_link_supplement_sha256':None,'static_link_recovery_sha256':None,'verified_inputs_sha256':runtime.canonical({'gpu_build':b,'gpu_aggregate':a})})
 def test_incremental_projection_uses_child_not_parent(self):
  b=proof();b['parent_producer']={'sha256':'f'*64};p=runtime.static_projection(b,aggregate(b))
  self.assertEqual(p['build_receipt_sha256'],'c'*64)
 def test_parent_aggregate_cannot_be_retagged(self):
  with self.assertRaises(runtime.GpuProofUnavailable):runtime.static_projection(proof(),aggregate(proof(2)))
 def test_incremental_aggregate_delegates_only_shared_owner(self):
  owner=SimpleNamespace(verify_aggregate=Mock(return_value='verified'))
  with patch.object(agg,'module',return_value=owner) as loaded:
   self.assertEqual(agg.verify_aggregate(Path('C:/destination'),proof(),'core'),'verified')
  loaded.assert_called_once_with('cuda-static-incremental');owner.verify_aggregate.assert_called_once()
 def test_incremental_relink_refused_before_any_io(self):
  with patch.object(agg,'module') as loaded,patch.object(Path,'exists') as exists:
   with self.assertRaisesRegex(ValueError,'consumer relink refused'):agg.aggregate(Path('C:/destination'),proof(),'core')
  loaded.assert_not_called();exists.assert_not_called()
 def handoff(self):
  b=proof()
  with patch.object(env,'_load',return_value=runtime):
   return env.incremental_handoff_value(Path('C:/destination'),Path('C:/child'),Path('C:/proposal'),'c'*64,b,aggregate(b))
 def test_environment5_exact_shape_and_no_activation(self):
  v=self.handoff();self.assertEqual(v['schema'],'airk.cuda-static-environment/5')
  self.assertEqual(set(v['request']),{'destination','incremental_root','incremental_proposal','receipt_sha256'})
  self.assertEqual(set(v['environment']),{'LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_SKIP_DOWNLOAD','ORT_PREFER_DYNAMIC_LINK','ORT_LIB_LOCATION','ORT_LIB_PROFILE'})
  self.assertFalse(v['activation_pins_available'])
 def test_handoff_rejects_old_proof_or_wrong_child_sha(self):
  for b,sha in [(proof(2),'c'*64),(proof(),'f'*64)]:
   with self.assertRaises(ValueError):env.incremental_handoff_value('d','r','p',sha,b,aggregate(b))
 def verify(self,v,deadline=None):
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),digest=lambda p:'a'*64,read_json=lambda p:v)
  owner=SimpleNamespace(verify_incremental_bundle=Mock(return_value={'build':proof(),'aggregate':aggregate(proof())}))
  ag=SimpleNamespace(verify_aggregate=Mock(return_value=aggregate(proof())));owner.aggregate_owner=ag
  def module(name):return {'cuda-static-runtime':runtime,'cuda-static-aggregate':ag,'cuda-static-incremental':owner}[name]
  with patch.object(env,'_load',side_effect=module),patch.object(runtime,'load',return_value=core):
   result=env.verify_handoff('C:/env','a'*64,'C:/repo',deadline=deadline)
  return result,owner
 def test_verify_environment5_reopens_exact_shared_owner(self):
  result,owner=self.verify(self.handoff());self.assertEqual(result['gpu_build'],proof())
  owner.verify_incremental_bundle.assert_called_once_with(Path('C:/destination'),'C:\\child','C:\\proposal','c'*64,'C:/repo')
 def test_environment5_rejects_extra_or_old_request_keys(self):
  for key in ['producer_file','composite_root','unexpected']:
   v=self.handoff();v['request'][key]='C:/old'
   with self.assertRaises(runtime.GpuProofUnavailable):self.verify(v)
 def test_environment5_stale_build_aggregate_or_environment_refuses(self):
  for section in ['build','aggregate','environment']:
   v=self.handoff();v[section]['drift']='changed'
   with self.assertRaises(runtime.GpuProofUnavailable):self.verify(v)
 def test_cli_mixed_or_incomplete_incremental_mode_refuses(self):
  core=SimpleNamespace(plain=lambda p:p)
  rt=SimpleNamespace(load=lambda *a:core)
  for extra in [['--incremental-root','C:/child'],['--incremental-root','C:/child','--incremental-proposal','C:/proposal','--checkpointed-root','C:/old']]:
   argv=['provision','--destination','C:/destination','--receipt-sha256','c'*64]+extra
   with patch.object(sys,'argv',argv),patch.object(cli,'load',return_value=rt):
    with self.assertRaisesRegex(ValueError,'complete incremental mode'):cli.main()
 def test_cli_projects_incremental_handoff_from_one_bundle_without_relink(self):
  b=proof()
  a=aggregate(b)
  destination=Path('C:/destination')
  owner=SimpleNamespace(verify_incremental_bundle=Mock(return_value={'build':b,'aggregate':a}))
  legacy=SimpleNamespace(verify_aggregate=Mock(),aggregate=Mock())
  core=SimpleNamespace(plain=lambda path:path)
  runtime_loader=SimpleNamespace(load=lambda *args:core)
  modules={'cuda-static-runtime':runtime_loader,'cuda-static-aggregate':legacy,
           'cuda-runtime-environment':env,'cuda-static-incremental':owner}
  argv=['provision','--repo-root','C:/repo','--destination',str(destination),
        '--incremental-root','C:/child','--incremental-proposal','C:/proposal',
        '--receipt-sha256','c'*64]
  with patch.object(sys,'argv',argv),patch.object(cli,'load',side_effect=modules.__getitem__),patch('builtins.print') as printed:
   cli.main()
   self.assertEqual(json.loads(printed.call_args.args[0]),self.handoff())
   with patch.object(sys,'argv',[*argv,'--aggregate']),self.assertRaisesRegex(ValueError,'consumer relink refused'):
    cli.main()
  owner.verify_incremental_bundle.assert_called_once_with(
   destination,Path('C:/child'),Path('C:/proposal'),'c'*64,Path('C:/repo'))
  legacy.verify_aggregate.assert_not_called()
  legacy.aggregate.assert_not_called()

 def test_staging_verifier_refusal_precedes_mutation(self):
  rt=SimpleNamespace(load=lambda *a:object());owner=SimpleNamespace(verify_incremental_bundle=Mock(side_effect=ValueError('producer refused')))
  with patch.object(stage,'load',side_effect=lambda name:rt if name=='cuda-static-runtime' else owner),patch.object(Path,'mkdir') as mkdir:
   with self.assertRaisesRegex(ValueError,'producer refused'):stage.stage_incremental_providers('d',{}, {},'o','i','p','sha','repo')
  mkdir.assert_not_called()

 def test_staging_guards_every_creation_and_audits_before_consumption(self):
  b=proof();b['build_receipt']={'path':'C:/child/raw.json','sha256':'c'*64,'bytes':3}
  b['recipe']={'path':'C:/proposal/recipe.json','sha256':'r'*64,'bytes':3}
  b['provider_dlls']=[{'path':'b/onnxruntime_providers_cuda.dll','sha256':'a'*64,'bytes':3},{'path':'b/onnxruntime_providers_shared.dll','sha256':'b'*64,'bytes':3}]
  a=aggregate(b);events=[]
  policy={'path':'C:/approved/fresh','root':'C:/approved','expected_sid':'human','guard':{'path':'C:/guard.ps1','sha256':'g'*64,'bytes':3},'powershell':{'path':'C:/pwsh.exe','sha256':'p'*64,'bytes':3}}
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),verify_file=lambda p,r:events.append(('verify',str(p))),read_json=lambda p:{'provider_stage':policy,'parent_context':{'destination':'C:/original-cu8'}},child=lambda d,p:Path(d)/p,digest=lambda p:'a'*64)
  rt=SimpleNamespace(load=lambda *a:core,canonical=runtime.canonical)
  owner=SimpleNamespace(verify_incremental_bundle=Mock(return_value={'build':b,'aggregate':a}))
  aggregate_owner=SimpleNamespace(verify_aggregate=Mock(return_value=a))
  def module(name):return {'cuda-static-runtime':rt,'cuda-static-incremental':owner,'cuda-static-aggregate':aggregate_owner,'cuda-runtime-environment':env}[name]
  def opened(path,mode,*args,**kwargs):events.append(('open',str(path),mode));return io.BytesIO(b'abc' if mode=='rb' else b'')
  import subprocess
  with patch.object(stage,'load',side_effect=module),patch.object(Path,'exists',return_value=False),patch.object(Path,'mkdir',side_effect=lambda:events.append(('mkdir',))),patch.object(Path,'open',opened),patch.object(Path,'stat',return_value=SimpleNamespace(st_size=3)),patch.object(subprocess,'run',side_effect=lambda argv,**kw:events.append(('audit',argv[-1]))):
   result=stage.stage_incremental_providers(Path('C:/destination'),b,a,Path('C:/approved/fresh'),'i','p','sha','repo')
  self.assertFalse(result['qualification']);self.assertFalse(result['complete_cuda_runtime'])
  calls=owner.verify_incremental_bundle.call_args_list
  self.assertEqual(len(calls),2)
  expected=(Path('C:/destination'),'i','p','sha','repo')
  self.assertEqual([call.args for call in calls],[expected,expected])
  self.assertEqual(calls[0].kwargs,calls[1].kwargs)
  self.assertIn('deadline',calls[0].kwargs)
  aggregate_owner.verify_aggregate.assert_not_called()
  self.assertTrue(any(x[0]=='open' and x[1]==str(Path('C:/original-cu8/b/onnxruntime_providers_cuda.dll')) for x in events))
  self.assertEqual(sum(x[0]=='audit' for x in events),10)
  self.assertEqual([x[2] for x in events if x[0]=='open' and x[2]=='xb'],['xb']*4)
  for i,event in enumerate(events):
   if event[0]=='open' and event[2]=='xb':
    later=next(x for x in events[i+1:] if x[0]=='audit')
    self.assertIn('Assert-AirkCreatedOutput',later[1])


 def test_actual_environment_path_uses_authenticated_fresh_aggregate_pin(self):
  b=proof();a=aggregate(b)
  value=env.environment(Path('C:/destination'),b,a)
  self.assertEqual(value['ORT_LIB_LOCATION'],str(Path('C:/destination/aggregate')))
  for change in ['old-root','mismatch-sha','old-output']:
   changed=copy.deepcopy(b);altered=copy.deepcopy(a)
   if change=='old-root':changed['new_aggregate']['path']='C:/cu8/gpu-aggregate/onnxruntime.lib'
   elif change=='mismatch-sha':altered['output']['sha256']='f'*64
   else:altered['output']['path']='gpu-aggregate/onnxruntime.lib'
   altered['producer_proof_sha256']=runtime.canonical(changed)
   with self.assertRaises(ValueError):env.environment(Path('C:/destination'),changed,altered)
 def test_actual_legacy_environment_locations_are_unchanged(self):
  for version in [1,2]:
   b=proof(version);self.assertEqual(env.environment(Path('C:/old'),b,aggregate(b))['ORT_LIB_LOCATION'],str(Path('C:/old/gpu-aggregate')))
 def test_handoff_write_guard_order_and_refusal_no_recursive_mkdir(self):
  events=[];core=SimpleNamespace(plain=lambda p:Path(p))
  with patch.object(env,'incremental_output_policy',return_value={}),patch.object(env,'audit_incremental_output',side_effect=lambda p,f,c,created=False,timeout=10:events.append(('audit',created))),patch.object(Path,'exists',return_value=False),patch.object(Path,'mkdir') as mkdir,patch.object(Path,'open',side_effect=lambda *a,**kw:(events.append(('write',a[0])),io.BytesIO())[1]):
   env.write_incremental_handoff(Path('C:/fresh.json'),{},proof(),core)
  self.assertEqual(events,[('audit',False),('write','xb'),('audit',True)]);mkdir.assert_not_called()
  with patch.object(env,'incremental_output_policy',return_value={}),patch.object(env,'audit_incremental_output',side_effect=ValueError('guard refused')),patch.object(Path,'exists',return_value=False),patch.object(Path,'open') as opened:
   with self.assertRaisesRegex(ValueError,'guard refused'):env.write_incremental_handoff(Path('C:/fresh.json'),{},proof(),core)
  opened.assert_not_called()
 def test_guard_pins_both_interpreter_and_helper_and_sets_timeout(self):
  policy={'root':'C:/approved','expected_sid':'human','guard':{'path':'C:/guard.ps1'},'powershell':{'path':'C:/pwsh.exe'}}
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),verify_file=Mock())
  with patch.object(env.subprocess,'run') as called:env.audit_incremental_output(policy,Path('C:/approved/fresh'),core,True,2.5)
  self.assertEqual(core.verify_file.call_count,2);self.assertEqual(called.call_args.kwargs['timeout'],2.5);self.assertEqual(called.call_args.args[0][0],'C:/pwsh.exe')
  with self.assertRaises(ValueError):env.audit_incremental_output(policy,'p',core,False,float('inf'))


 def test_staging_primary_failure_survives_secondary_receipt_failure(self):
  b=proof();b['recipe']={'path':'C:/recipe.json'};b['build_receipt']={'path':'C:/raw.json','sha256':'c'*64}
  b['provider_dlls']=[{'path':'b/onnxruntime_providers_cuda.dll'},{'path':'b/onnxruntime_providers_shared.dll'}]
  def verify(path,row):
   if str(path).endswith('onnxruntime_providers_cuda.dll'):raise ValueError('primary source drift')
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),child=lambda d,p:Path(d)/p,verify_file=verify,read_json=lambda p:{'parent_context':{'destination':'C:/original-cu8'}})
  rt=SimpleNamespace(load=lambda *a:core,canonical=runtime.canonical)
  owner=SimpleNamespace(verify_incremental_bundle=lambda *a,**kw:{'build':b,'aggregate':aggregate(b)})
  def audit(policy,path,*args):
   if str(path).endswith('staging-receipt.json'):raise OSError('secondary audit failed')
  guards=SimpleNamespace(incremental_output_policy=lambda *a:{},audit_incremental_output=audit)
  modules={'cuda-static-runtime':rt,'cuda-static-incremental':owner,'cuda-runtime-environment':guards,'cuda-static-aggregate':SimpleNamespace(verify_aggregate=lambda *a,**kw:aggregate(b))}
  with patch.object(stage,'load',side_effect=modules.__getitem__),patch.object(Path,'exists',return_value=False),patch.object(Path,'mkdir'):
   with self.assertRaisesRegex(ValueError,'primary source drift') as caught:stage.stage_incremental_providers(Path('C:/destination'),b,aggregate(b),Path('C:/fresh'),'i','p','s','repo')
  self.assertIn('secondary audit failed',caught.exception.__notes__[0])


 def test_handoff_late_guard_return_refuses_before_write(self):
  import time
  core=SimpleNamespace(plain=lambda p:Path(p))
  with patch.object(env,'incremental_output_policy',return_value={}),patch.object(env,'audit_incremental_output'),patch.object(Path,'exists',return_value=False),patch.object(Path,'open') as opened,patch.object(time,'monotonic',side_effect=[0,299,300]):
   with self.assertRaises(TimeoutError):env.write_incremental_handoff(Path('C:/fresh.json'),{},proof(),core)
  opened.assert_not_called()
 def test_stage_late_guard_return_refuses_before_mkdir_or_copy(self):
  import time
  b=proof();b['recipe']={'path':'C:/recipe.json'};b['build_receipt']={'path':'C:/raw.json','sha256':'c'*64}
  b['provider_dlls']=[{'path':'b/onnxruntime_providers_cuda.dll'},{'path':'b/onnxruntime_providers_shared.dll'}]
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),child=lambda d,p:Path(d)/p,verify_file=lambda *a:None,read_json=lambda p:{'parent_context':{'destination':'C:/original-cu8'}})
  rt=SimpleNamespace(load=lambda *a:core,canonical=runtime.canonical)
  owner=SimpleNamespace(verify_incremental_bundle=lambda *a,**kw:{'build':b,'aggregate':aggregate(b)})
  guards=SimpleNamespace(incremental_output_policy=lambda *a:{},audit_incremental_output=lambda *a:None)
  modules={'cuda-static-runtime':rt,'cuda-static-incremental':owner,'cuda-runtime-environment':guards,'cuda-static-aggregate':SimpleNamespace(verify_aggregate=lambda *a,**kw:aggregate(b))}
  for times,created in [([0,299,300],False),([0,1,2,3,4,299,300],True)]:
   clock=iter(times)
   with patch.object(stage,'load',side_effect=modules.__getitem__),patch.object(Path,'exists',return_value=False),patch.object(Path,'mkdir') as mkdir,patch.object(Path,'open') as opened,patch.object(time,'monotonic',side_effect=lambda:next(clock,301)):
    with self.assertRaises(TimeoutError):stage.stage_incremental_providers(Path('C:/destination'),b,aggregate(b),Path('C:/fresh'),'i','p','s','repo')
   self.assertEqual(mkdir.call_count,1 if created else 0);opened.assert_not_called()


 def test_checkpoint_aggregate_request_owner_never_imports_ignored_code(self):
  maintained=object();checkpoint=SimpleNamespace(execution_context=lambda *a:(Path('C:/historical-r12'),{}),verification=Mock(return_value=maintained));rt=SimpleNamespace(load=Mock(side_effect=AssertionError('ignored import')))
  with patch.object(agg,'module',return_value=checkpoint):
   proposal,child=agg.aggregate_tree_owner(proof(2),'core',rt)
  self.assertEqual(proposal,Path('C:/historical-r12'));self.assertIs(child,maintained)
  checkpoint.verification.assert_called_once_with('materialized');rt.load.assert_not_called()
 def test_legacy_aggregate_request_owner_dispatch_retained(self):
  rt=SimpleNamespace(load=Mock(return_value='historical'));b=proof(1);b['ancestry']={'r12_proposal':'C:/historical'}
  self.assertEqual(agg.aggregate_tree_owner(b,'core',rt),(Path('C:/historical'),'historical'))
  rt.load.assert_called_once_with(Path('C:/historical/continue_cuda_runtime.py'),'r12_aggregate_tree_helper')


 def test_handoff_verification_propagates_original_deadline(self):
  with patch.object(env.time,'monotonic',return_value=10):
   result,owner=self.verify(self.handoff(),deadline=42)
  self.assertEqual(owner.verify_incremental_bundle.call_args.kwargs['deadline'],42)
  owner.aggregate_owner.verify_aggregate.assert_not_called()
  self.assertEqual(result['gpu_build'],proof())
 def test_handoff_expiry_refuses_before_loading_verifiers(self):
  with patch.object(env.time,'monotonic',return_value=42),patch.object(env,'_load') as loaded:
   with self.assertRaises(TimeoutError):env.verify_handoff('p','h','repo',deadline=42)
  loaded.assert_not_called()
 def test_handoff_late_shared_verifier_refuses_before_aggregate(self):
  v=self.handoff();clock=[1]
  core=SimpleNamespace(plain=lambda p,**kw:Path(p),digest=lambda p:'a'*64,read_json=lambda p:v)
  def late(*a,**kw):clock[0]=42;return proof()
  owner=SimpleNamespace(verify_incremental_bundle=Mock(side_effect=late));ag=SimpleNamespace(verify_aggregate=Mock())
  modules={'cuda-static-runtime':runtime,'cuda-static-aggregate':ag,'cuda-static-incremental':owner}
  with patch.object(env.time,'monotonic',side_effect=lambda:clock[0]),patch.object(env,'_load',side_effect=modules.__getitem__),patch.object(runtime,'load',return_value=core):
   with self.assertRaises(TimeoutError):env.verify_handoff('C:/env','a'*64,'C:/repo',deadline=42)
  ag.verify_aggregate.assert_not_called()


 def test_malformed_verified_bundle_refuses_without_second_verification(self):
  v=self.handoff();core=SimpleNamespace(plain=lambda p,**kw:Path(p),digest=lambda p:'a'*64,read_json=lambda p:v)
  for bundle in [[],{'build':proof()},{'build':proof(),'aggregate':aggregate(proof()),'extra':True}]:
   owner=SimpleNamespace(verify_incremental_bundle=Mock(return_value=bundle));ag=SimpleNamespace(verify_aggregate=Mock())
   modules={'cuda-static-runtime':runtime,'cuda-static-aggregate':ag,'cuda-static-incremental':owner}
   with patch.object(env,'_load',side_effect=modules.__getitem__),patch.object(runtime,'load',return_value=core):
    with self.assertRaises(runtime.GpuProofUnavailable):env.verify_handoff('C:/env','a'*64,'C:/repo')
   owner.verify_incremental_bundle.assert_called_once();ag.verify_aggregate.assert_not_called()

if __name__=='__main__':unittest.main()
