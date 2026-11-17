# SPDX-License-Identifier: Apache-2.0
"""Cheap proposal-only structural controls. No compiler, Ninja or writes."""
import hashlib, json, pathlib
HERE=pathlib.Path(__file__).resolve().parent
r=json.loads((HERE/'recipe.json').read_bytes());out=pathlib.Path(r['fresh_output']);checks=[]
def check(name,condition):
 if not condition: raise ValueError(name)
 checks.append(name)
def frozen(pin):
 p=pathlib.Path(pin['path']);h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(4<<20):h.update(b)
 return p.stat().st_size==pin['bytes'] and h.hexdigest()==pin['sha256']
check('not-executable-and-fresh',r['execution_authorized'] is False and not out.exists())
check('closed-parent-identities',r['old_producer']['sha256']=='f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178' and r['old_aggregate']['sha256']=='042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00')
check('exact-input-counts',len(r['compiler_dependencies'])==482 and len(r['session_objects'])==40 and len(r['aggregate_libraries'])==96)
check('only-one-session-object-replaced',len(r['session_input_paths'])==40 and len(set(r['session_input_paths']))==40 and set(r['session_input_paths'])-set(x['path'] for x in r['session_objects'])=={str(out/'objects/onnxruntime_c_api.cc.obj')} and len(set(x['path'] for x in r['session_objects'])-set(r['session_input_paths']))==1)
check('only-one-aggregate-library-replaced',len(r['aggregate_input_paths'])==96 and len(set(r['aggregate_input_paths']))==96 and set(r['aggregate_input_paths'])-set(x['path'] for x in r['aggregate_libraries'])=={str(out/'session/onnxruntime_session.lib')} and len(set(x['path'] for x in r['aggregate_libraries'])-set(r['aggregate_input_paths']))==1)
original=r['original_compile_argv'];actual=r['commands']['compile'];quote='/I'+str(pathlib.Path(r['base_source']['path']).parent)
expected=[actual[0]]+[('/Fo'+str(out/'objects/onnxruntime_c_api.cc.obj') if x.startswith('/Fo') else '/Fd'+str(out/'objects/onnxruntime_session.pdb') if x.startswith('/Fd') else str(out/'source/onnxruntime_c_api.cc') if x==r['base_source']['path'] else x) for x in original[1:]]
expected.insert(next(i for i,x in enumerate(expected) if x.startswith(('-I','/I'))),quote)
check('compile-exact-approved-relocations-only',actual==expected)
check('quote-parent-precedes-original-include-roots',actual.index(quote)==next(i for i,x in enumerate(actual) if x.startswith(('-I','/I'))))
check('fresh-output-flags',sum(x.startswith('/Fo') for x in actual)==1 and sum(x.startswith('/Fd') for x in actual)==1 and r['commands']['session'][3]=='/OUT:'+str(out/'session/onnxruntime_session.lib') and r['commands']['aggregate'][2]=='/OUT:'+str(out/'aggregate/onnxruntime.lib'))
check('frozen-tool-env',r['environment']['VCTOOLSVERSION']=='14.44.35207' and r['environment']['WINDOWSSDKVERSION']=='10.0.26100.0\\' and r['environment']['VSLANG']=='1033' and r['environment']['CL']==r['environment']['_CL_']=='' and pathlib.Path(r['environment']['TEMP'])==out/'tmp' and pathlib.Path(r['environment']['TMP'])==out/'tmp')
check('export-pair',r['required_exports']==['airk_ort_collect_final_placement_v1','airk_ort_collect_final_graph_metadata_v1'])
for row,paths in zip(r['response_files'],[r['session_input_paths'],r['aggregate_input_paths']]):
 raw=pathlib.Path(row['proposal_path']).read_bytes();expected=('\r\n'.join('"'+p+'"' for p in paths)+'\r\n').encode()
 check('response-'+row['name'],raw==expected and len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'])
check('no-fabricated-new-results',all(v is None for v in r['new_result_pins'].values()))
check('base-and-ninja-still-frozen',all(frozen(r[k]) for k in ['base_source','ninja_graph','ninja_rules','cmake_cache','old_producer']))
if r['source_patch'] is not None:
 check('approved-new-tu-frozen',frozen(r['source_patch']['after']) and frozen(r['source_patch']['patch']))
print(json.dumps({'passed':len(checks),'checks':checks,'source_patch_bound':r['source_patch'] is not None,'build_executed':False,'actual_include_export_proofs':False}))
