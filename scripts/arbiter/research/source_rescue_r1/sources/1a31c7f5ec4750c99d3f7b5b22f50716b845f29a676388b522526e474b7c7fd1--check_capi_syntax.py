# SPDX-License-Identifier: Apache-2.0
"""Root-owned R10 syntax check with exact R8 scrub policy; no full build."""
from pathlib import Path
import ast,datetime,hashlib,importlib.util,json,argparse,os,time
HERE=Path(__file__).resolve().parent
R9=HERE.parent/'gpu-provision-owner-proposal-r9'
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def main():
 a=argparse.ArgumentParser();a.add_argument('--execute-syntax',action='store_true');a.add_argument('--approved-script-sha256',required=True);a.add_argument('--approved-input-sha256',required=True);a.add_argument('--tree-control-receipt',type=Path,required=True);a.add_argument('--tree-control-receipt-sha256',required=True);v=a.parse_args()
 if not v.execute_syntax or digest(__file__)!=v.approved_script_sha256 or digest(HERE/'input-pins.json')!=v.approved_input_sha256:raise ValueError('explicit reviewed R10 syntax inputs required')
 pins=json.loads((HERE/'input-pins.json').read_bytes())
 for item in pins:
  if Path(item['path']).stat().st_size!=item['bytes'] or digest(item['path'])!=item['sha256']:raise ValueError('frozen R10 syntax input differs')
 req=json.loads((R9/'continuation-request.json').read_bytes());recipe=load(req['recipe']['path'],'reviewed_r8_syntax');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=recipe.cpu_owner(lock);parent=o.read_json(req['parent_receipt']['path']);control_path=o.plain(v.tree_control_receipt,regular=True)
 if digest(control_path)!=v.tree_control_receipt_sha256:raise ValueError('actual tree witness receipt differs')
 controls=o.read_json(control_path)
 if controls.get('status')!='PASS_PROCESS_TREE_WITNESSES_ONLY' or controls.get('input_pins_sha256')!=v.approved_input_sha256 or controls.get('script_sha256')!=digest(HERE/'run_tree_controls.py'):raise ValueError('actual reviewed tree witnesses required before syntax')
 cases=controls.get('cases',[])
 if len(cases)!=2 or [case.get('mode') for case in cases]!=['normal','linger'] or any(case.get('passed') is not True or not o.successful(case.get('owner',{})) for case in cases) or cases[1].get('parent_exit_observed_while_child_alive_and_wrapper_pending') is not True:raise ValueError('actual natural normal/lingering tree controls incomplete')
 card=o.read_json(req['syntax_card']['path']);wall=o.read_json(R9/'wall-origin.json')
 for item in pins:o.verify_file(o.plain(item['path'],regular=True),item)
 o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file'])
 cutoff=datetime.datetime.fromisoformat(wall['absolute_cutoff_utc']);clock_start=time.monotonic();observed=datetime.datetime.now(datetime.timezone.utc);deadline=clock_start+min(120,(cutoff-observed).total_seconds())
 if observed<datetime.datetime.fromisoformat(wall['prepared_utc']) or deadline<=clock_start:raise ValueError('original absolute wall cutoff expired/clock moved backwards')
 output=recipe.PARENT/'capi-syntax-r11';identity_lock=dict(lock);identity_lock['build_path']={'fresh_child':output.name};policy=recipe.identity_and_parent(o,output,identity_lock);output.mkdir()
 receipt={'schema':'airk.private.cuda-capi-syntax-result/1','status':'RUNNING','request_sha256':digest(R9/'continuation-request.json'),'card_sha256':digest(req['syntax_card']['path']),'source_sha256':req['after']['sha256'],'policy':policy,'process_tree_owner':True,'tree_control_receipt':{'path':str(control_path),'bytes':control_path.stat().st_size,'sha256':v.tree_control_receipt_sha256},'r9_failed_syntax_preserved':pins[-1],'environment_policy_source_sha256':digest(HERE/'environment-source.json'),'original_absolute_cutoff_utc':cutoff.isoformat(),'observed_utc':observed.isoformat(),'monotonic_deadline':deadline}
 start=time.monotonic()
 try:
  for item in parent['toolchain'].values():
   if isinstance(item,dict) and {'path','bytes','sha256'}.issubset(item):o.verify_file(o.plain(item['path'],regular=True),item)
  if digest(card['argv'][0])!=card['compiler']['sha256']:raise ValueError('captured compiler spelling differs')
  source=o.read_json(HERE/'environment-source.json');tree=ast.parse(Path(source['producer_path']).read_bytes());owner_main=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='main');wanted=ast.parse(source['raw_environment_block']).body
  # Require exact contiguous original AST statements; no guessed approximate recreation.
  original_text=Path(source['producer_path']).read_text().splitlines();raw='\n'.join(original_text[source['first_line']-1:source['last_line']]);import textwrap
  if textwrap.dedent(raw)!=source['raw_environment_block']:raise ValueError('original environment source region differs')
  env_owner=load(HERE/'producer_environment.py','exact_producer_env');helper_tree=ast.parse((HERE/'producer_environment.py').read_bytes());actual=next(n for n in helper_tree.body if isinstance(n,ast.FunctionDef) and n.name=='producer_environment').body[:-1]
  if [ast.dump(n,include_attributes=False) for n in actual]!=[ast.dump(n,include_attributes=False) for n in wanted]:raise ValueError('environment helper differs from original producer policy')
  environment=env_owner.producer_environment(output,recipe.CUDA)
  receipt['environment_sha256']=hashlib.sha256(json.dumps(environment,sort_keys=True,separators=(',',':')).encode()).hexdigest();receipt['environment_policy_equivalent_to_R8']=True;receipt['full_original_environment_equal']='unknown: original inherited values were not recorded';receipt['controlled_environment']={k:environment[k] for k in ['PATH','TEMP','TMP','TMPDIR','PYTHONNOUSERSITE','CUDA_CACHE_PATH']};receipt['new_telemetry_flags']=False
  remaining=min(deadline-time.monotonic(),(cutoff-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
  if remaining<=0:raise TimeoutError('syntax/original wall cutoff expired before compiler')
  actual=o.windows_command(card['argv'],parent['toolchain'],output,'capi-syntax-r11');tree_owner=load(HERE/'process_tree.py','reviewed_tree_owner');receipt['original_command_argv']=actual;receipt['command']=tree_owner.run_tree(o,actual,Path(card['cwd']),output,'capi-syntax-r11',remaining,environment)
  if not o.successful(receipt['command']):raise ValueError('corrected environment syntax compile not naturally settled')
  for item in pins:o.verify_file(o.plain(item['path'],regular=True),item)
  if digest(control_path)!=v.tree_control_receipt_sha256:raise ValueError('actual control receipt drifted')
  if time.monotonic()>=deadline or datetime.datetime.now(datetime.timezone.utc)>=cutoff:raise TimeoutError('syntax/original wall cutoff expired during final closure')
  receipt['status']='PASS_SYNTAX_ONLY'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-start;recipe.write(output/'receipt.json',receipt)
 print(json.dumps({'output':str(output),'status':receipt['status']}))
if __name__=='__main__':main()
