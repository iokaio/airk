# SPDX-License-Identifier: Apache-2.0
"""GPU-only genuine static aggregate plan/verification; CPU owner unchanged."""
from pathlib import Path
import importlib.util,json,os,time,datetime
ROOT=Path(__file__).resolve().parents[1]
FINAL_EXPORT='airk_ort_collect_final_placement_v1'
MAX_ARCHIVE=2*1024**3

def module(name):
 path=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def plan(destination,build,core):
 runtime=module('cuda-static-runtime');archive=module('cuda-static-archive');destination=core.plain(destination)
 ninja=core.plain(destination/'b/Release/build.ninja',regular=True);core.verify_file(ninja,build['ninja'])
 roots=['onnxruntime_'+name+'.lib' for name in core.STATIC_COMPONENTS]+['_deps/re2-build/re2.lib','_deps/pytorch_cpuinfo-build/cpuinfo.lib']
 graph=archive.runtime_closure(ninja.read_text(encoding='utf8'),roots)
 if build.get('ancestry',{}).get('receipt_schema')=='airk.private.cuda-runtime-continuation/2' and len(graph['static_inputs'])!=96:raise ValueError('exact observed R12 static target closure must contain96 archives')
 recorded={r['path']:r for r in build['libraries']};rows=[];exports=set()
 for relative in graph['static_inputs']:
  p=core.child(destination,'b/Release/'+relative);key=p.relative_to(destination).as_posix()
  if key not in recorded:raise ValueError('Ninja-selected archive absent from actual producer output census')
  row=recorded[key];core.verify_file(p,row);observed=archive.inspect_archive(p)
  if observed['kind']!='static':raise ValueError('import/mixed library cannot enter static core aggregate')
  exports.update(observed['export_directives']);rows.append({**row,'coff':observed})
 if FINAL_EXPORT not in exports:raise ValueError('actual selected core objects lack required final-placement export directive')
 if sum(r['bytes'] for r in rows)>MAX_ARCHIVE:raise ValueError('GPU aggregate exceeds2GiB bound')
 imports=[]
 for dll,metadata in graph['provider_modules'].items():
  relative=metadata['import_library']
  if relative is None:continue # MODULE provider has no fictitious import archive.
  key='b/Release/'+relative
  if key not in recorded:raise ValueError('actual provider import library missing')
  row=recorded[key];p=core.child(destination,key);core.verify_file(p,row);observed=archive.inspect_archive(p)
  if observed['kind']!='import_or_mixed' or any(n!=dll.lower() for n in observed['imported_dlls']):raise ValueError('provider library is not its actual DLL import interface')
  imports.append({**row,'module':dll,'coff':observed})
 return {'schema':'airk.cuda-static-aggregate-plan/1','target':runtime.TARGET,'ort_version':runtime.VERSION,'consumer':'ort-sys-2.0.0-rc.10','producer_proof_sha256':runtime.canonical(build),'ninja_sha256':core.digest(ninja),'graph':graph,'inputs':rows,'provider_import_libraries_excluded':imports,'required_export_directive':FINAL_EXPORT,'qualification':False}

def request(destination,build,selected,core):
 compiler=Path(build['recorded_compiler']['path']);tool=core.plain(compiler.with_name('lib.exe'),regular=True)
 response=destination/'gpu-aggregate/inputs.rsp';output=destination/'gpu-aggregate/onnxruntime.lib'
 paths=[str(core.child(destination,r['path'])) for r in selected['inputs']]
 if any(any(c in path for c in '\r\n"') for path in paths):raise ValueError('unsafe native archive response path')
 text='\n'.join('"'+path+'"' for path in paths)+'\n'
 argv=[str(tool),'/NOLOGO','/OUT:'+str(output),'@'+str(response)]
 return {'tool':{'path':str(tool),'bytes':tool.stat().st_size,'sha256':core.digest(tool)},'response':response,'text':text,'argv':argv,'output':output}

def verify_aggregate(destination,build,core):
 runtime=module('cuda-static-runtime');archive=module('cuda-static-archive');receipt=core.read_json(destination/'gpu-static-aggregate-receipt.json')
 selected=plan(destination,build,core);req=request(destination,build,selected,core)
 if receipt.get('status')!='PASS_GPU_AGGREGATE_ONLY_UNQUALIFIED' or receipt.get('plan')!=selected or not core.successful(receipt.get('command',{})):raise ValueError('actual GPU aggregate command/provenance missing')
 expected_command=[str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(destination/'commands/gpu-aggregate.cmd')]
 if build.get('ancestry',{}).get('receipt_schema')=='airk.private.cuda-runtime-continuation/2':
  if receipt.get('execution_mode')!='r12-complete-owned-tree' or receipt.get('elapsed_seconds',301)>300:raise ValueError('actual bounded R12 aggregate tree proof missing')
  bridge=module('cuda-static-continuation-r12');proposal=Path(build['ancestry']['r12_proposal']);child=runtime.load(proposal/'continue_cuda_runtime.py','r12_aggregate_tree_helper');cutoff=datetime.datetime.fromisoformat(receipt['observed_utc'])+datetime.timedelta(seconds=300)
  bridge.verify_tree_command(core,child,proposal,destination,destination,'gpu-aggregate',req['argv'],build['toolchain'],receipt['command'],receipt,cutoff)
 elif receipt.get('command',{}).get('argv')!=expected_command:raise ValueError('GPU aggregate actual owned command differs')
 if receipt.get('argv')!=req['argv'] or receipt.get('tool')!=req['tool']:raise ValueError('GPU aggregate tool/argument drift')
 if core.plain(req['response'],regular=True).read_bytes()!=req['text'].encode() or receipt.get('response_sha256')!=core.digest(req['response']):raise ValueError('GPU aggregate exact input response bytes differ')
 row=receipt['output'];core.verify_file(core.plain(req['output'],regular=True),row);observed=archive.inspect_archive(req['output'])
 archive.verify_member_closure([r['coff'] for r in selected['inputs']],observed)
 if observed!=receipt.get('coff') or observed['kind']!='static' or FINAL_EXPORT not in observed['export_directives']:raise ValueError('genuine GPU aggregate/import/export closure differs')
 if row['bytes']<=65536 or row['bytes']>MAX_ARCHIVE:raise ValueError('GPU aggregate size out of bounds')
 return {'schema':'airk.cuda-static-aggregate-proof/1','target':runtime.TARGET,'producer_proof_sha256':runtime.canonical(build),'receipt_sha256':core.digest(destination/'gpu-static-aggregate-receipt.json'),'output':row,'plan_sha256':runtime.canonical(selected),'coff':observed,'qualification':False}

def aggregate(destination,build,core):
 # Only call with a freshly reverified build proof from cuda-static-runtime.
 if (destination/'gpu-aggregate').exists() or (destination/'gpu-static-aggregate-attempt.json').exists():raise ValueError('existing aggregate preserved; no retry/overwrite')
 selected=plan(destination,build,core);req=request(destination,build,selected,core);(destination/'gpu-aggregate').mkdir();req['response'].write_bytes(req['text'].encode())
 receipt={'schema':'airk.cuda-static-aggregate-receipt/1','status':'RUNNING','plan':selected,'tool':req['tool'],'argv':req['argv'],'response_sha256':core.digest(req['response'])};core.write_json(destination/'gpu-static-aggregate-attempt.json',receipt);start=time.monotonic()
 try:
  actual=core.windows_command(req['argv'],build['toolchain'],destination,'gpu-aggregate')
  if build.get('ancestry',{}).get('receipt_schema')=='airk.private.cuda-runtime-continuation/2':
   proposal=Path(build['ancestry']['r12_proposal']);runtime=module('cuda-static-runtime');env_owner=runtime.load(proposal/'producer_environment.py','exact_r12_aggregate_environment');recipe=runtime.load(proposal/'continue_cuda_runtime.py','exact_r12_aggregate_helper');source_request=core.read_json(proposal/'continuation-request.json');r8=runtime.load(source_request['recipe']['path'],'r8_aggregate_input');environment=env_owner.producer_environment(destination/'gpu-aggregate',r8.CUDA);tree=runtime.load(proposal/'process_tree.py','r12_aggregate_tree')
   receipt['execution_mode']='r12-complete-owned-tree';receipt['observed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();receipt['environment_policy_source_sha256']=core.digest(proposal/'environment-source.json')
   remaining=300-(time.monotonic()-start)
   if remaining<=0:raise TimeoutError('aggregate original300s operation exhausted before archive tool')
   receipt['command']=tree.run_tree(core,actual,destination,destination,'gpu-aggregate',remaining,environment)
  else:receipt['command']=core.run_owned(actual,destination,destination/'logs/gpu-aggregate.log',300)
  if not core.successful(receipt['command']):raise ValueError('GPU aggregate command failed/unsettled')
  if plan(destination,build,core)!=selected:raise ValueError('GPU build inputs changed during aggregation')
  archive=module('cuda-static-archive');core.plain(req['output'],regular=True);receipt['output']={'path':req['output'].relative_to(destination).as_posix(),'bytes':req['output'].stat().st_size,'sha256':core.digest(req['output'])};receipt['coff']=archive.inspect_archive(req['output'])
  archive.verify_member_closure([r['coff'] for r in selected['inputs']],receipt['coff'])
  if receipt['coff']['kind']!='static' or FINAL_EXPORT not in receipt['coff']['export_directives']:raise ValueError('aggregate core lacks genuine static objects/export')
  if build.get('ancestry',{}).get('receipt_schema')=='airk.private.cuda-runtime-continuation/2' and time.monotonic()-start>=300:raise TimeoutError('aggregate operation expired during final object proof')
  receipt['status']='PASS_GPU_AGGREGATE_ONLY_UNQUALIFIED'
 except BaseException as e:
  receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(e).__name__+': '+str(e));raise
 finally:
  receipt['elapsed_seconds']=time.monotonic()-start;core.write_json(destination/'gpu-static-aggregate-receipt.json',receipt)
 return verify_aggregate(destination,build,core)
