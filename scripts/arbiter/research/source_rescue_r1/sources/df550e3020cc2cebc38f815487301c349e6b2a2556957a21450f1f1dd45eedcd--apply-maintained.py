# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,ast
N=chr(10)
root=pathlib.Path(r'D:\code\Github\airk')
ev=root/'artifacts/v190/final-plan/exec-20261116T052942Z'
app=ev/'gpu-maintained-tooling-application-r1'
r6=ev/'gpu-maintained-owner-proposal-r6'
br=ev/'gpu-runtime-builder-proposal-r1'
before={r['path']:r for r in json.loads((app/'ownership-before.json').read_text(encoding='utf-8-sig'))}
h=lambda b:hashlib.sha256(b).hexdigest()
targets={}
for row in json.loads((r6/'preparation.json').read_text(encoding='utf-8'))['files']:
    raw=(r6/row['path']).read_bytes()
    assert len(raw)==row['bytes'] and h(raw)==row['sha256'],row['path']
    if row['path'].startswith('scripts/'):targets[row['path']]=raw
for row in json.loads((br/'preparation.json').read_text(encoding='utf-8'))['sources']:
    raw=(br/'source'/row['path']).read_bytes()
    assert h(raw)==row['proposed_sha256'] and h((root/row['path']).read_bytes())==row['base_sha256'],row['path']
    targets[row['path']]=raw
for name in ('test_gpu_owners.py','test_native_package_adapter.py','test_package_consumers.py'):
    text=(r6/name).read_text(encoding='utf-8').replace('HERE=Path(__file__).resolve().parent','HERE=Path(__file__).resolve().parents[2]')
    if not text.startswith('# SPDX-'):text='# SPDX-License-Identifier: Apache-2.0'+N+text
    targets['scripts/tests/'+name]=text.encode()
targets['scripts/cuda-static-runtime/CONTRACT.md']=(r6/'CONTRACT.md').read_bytes()
targets['scripts/cuda-native-execution/CONTRACT.md']=(r6/'NN0-CONTRACT.md').read_bytes()
for name in targets:
    path=root/name
    if name in before:assert h(path.read_bytes())==before[name]['sha256'],('concurrent edit',name)
    else:assert not path.exists(),('new destination exists',name)
name='scripts/cuda-native-execution.py';text=targets[name].decode()
text=text.replace("'reference_parent.py':'775e32da732446919789490ca5a30b0b3d863f570b7bf1852d56e9e7c7391354'","'reference_parent_no_site_r2.py':'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d'")
text=text.replace("base/'reference_parent.py'","base/'reference_parent_no_site_r2.py'")
start=text.index('def finish_observation(');end=text.index(N+'def run(',start)
text=text[:start]+'''PROOF_LABELS=('projection','native_emission','stdout')
PROOF_CAPS={'projection':1<<20,'native_emission':1<<20,'stdout':64<<20}

def finish_observation(receipt,started,deadline,proofs,clock=time.monotonic,hash_file=pin):
 # Never skip a required hash callback: missing files and absent SHA rows fail.
 receipt['final_proof_success']=[]
 receipt['final_proof_errors']=[]
 receipt['final_proof_drift']={}
 if receipt.get('failure') is not None:receipt['complete']=False
 for label in PROOF_LABELS:
  try:
   path=proofs[label]
   row=hash_file(path)
   if not isinstance(row,dict) or set(row)!={'path','bytes','sha256'} or not isinstance(row['path'],str) or not Path(row['path']).is_absolute() or Path(row['path'])!=Path(path) or type(row['bytes']) is not int or not 0<=row['bytes']<=PROOF_CAPS[label] or not isinstance(row['sha256'],str) or len(row['sha256'])!=64 or any(c not in '0123456789abcdef' for c in row['sha256']):raise ValueError('mandatory final '+label+' hash callback returned no exact bounded SHA row')
   prior=receipt.get(label)
   if prior is not None and prior!=row:
    receipt['final_proof_drift'][label]={'original':prior,'observed':row}
    raise ValueError('mandatory final '+label+' bytes changed after native observation')
   receipt[label]=row
   receipt['final_proof_success'].append(label)
  except BaseException as exc:
   error={'label':label,'type':type(exc).__name__,'message':str(exc)}
   receipt['final_proof_errors'].append(error)
   if receipt.get('retention_failure') is None:receipt['retention_failure']=error
   if receipt.get('failure') is None:receipt['failure']=error
   receipt['complete']=False
 if receipt['final_proof_success']!=list(PROOF_LABELS):receipt['complete']=False
 # Sample the SAME original clock last, after all callbacks and postconditions.
 now=clock();receipt['elapsed_seconds']=now-started;receipt['whole_phase_expired']=now>=deadline
 if receipt['whole_phase_expired']:receipt['complete']=False;receipt['failure']=receipt['failure'] or {'type':'TimeoutError','message':'original NN0 phase expired during final retention'}
''' +text[end:]
old="finish_observation(receipt,started,deadline,log)"
assert text.count(old)==1
text=text.replace(old,"finish_observation(receipt,started,deadline,{'projection':output,'native_emission':output.with_suffix('.receipt.json'),'stdout':log})")
needle=" expected=[plan['native_image']['path'],'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']"
assert needle in text
text=text.replace(needle," if v.get('final_proof_success')!=list(PROOF_LABELS) or v.get('final_proof_errors')!=[] or v.get('final_proof_drift')!={}:raise ValueError('all mandatory original final output hash proofs required')"+N+needle)
targets[name]=text.encode()
name='scripts/cuda-native-execution/CONTRACT.md'
targets[name]+=b'''

Maintained integration correction: the exact existing helper is
reference_parent_no_site_r2.py (SHAfe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d),
with unchanged reference_owned.py2d7a and reference_worker.py9c04. No retained
helper is renamed, copied, edited or replaced.

All three mandatory final proofs (projection, native emission and stdout) must
invoke their actual hash callback and return an exact path/bytes/SHA row. Missing,
nonregular, oversized, unavailable or changed output makes the phase incomplete.
Every callback is attempted, original native/parent/settlement failures survive,
and final_proof_success/errors/drift retain actual outcomes. The SAME original
clock is sampled last, after callbacks and postconditions. Receipt encoding/write
is outside that measured elapsed scope. The accessor requires all three successful
proofs and independently reopens each pin. No skipped exists/is_file branch or
absent SHA row can turn an incomplete parent observation into a pass.
'''
def method(text,name,body):
    node=next(n for n in ast.walk(ast.parse(text)) if isinstance(n,ast.FunctionDef) and n.name==name)
    lines=text.splitlines(keepends=True)
    return ''.join(lines[:node.lineno-1])+body.rstrip()+N+''.join(lines[node.end_lineno:])
name='scripts/tests/test_gpu_owners.py';text=targets[name].decode()
text=text.replace('import importlib.util,struct,tempfile,unittest,hashlib','import importlib.util,struct,tempfile,unittest,hashlib'+N+'from types import SimpleNamespace'+N+'from unittest.mock import patch')
text=method(text,'test_missing_actual_gpu_receipt_is_unavailable',"""
 def test_missing_actual_gpu_receipt_is_unavailable(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   destination=Path(t)/'missing';producer=Path(t)/'producer.py';lock=Path(t)/'lock.json'
   expected={'producer.py':'c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a','lock.json':'549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d','provision-static-onnx-runtime.py':'a'*64,'static-onnx-runtime-lock.json':'b'*64}
   core=SimpleNamespace(plain=lambda p,regular=False:Path(p),digest=lambda p:expected[Path(p).name],read_json=lambda p,limit:{'cpu_owner_sha256':'a'*64,'cpu_lock_sha256':'b'*64})
   with patch.object(runtime,'load',return_value=core),self.assertRaisesRegex(runtime.GpuProofUnavailable,'actual completed GPU build receipt unavailable'):
    runtime.verify_build(destination,producer,lock,'0'*64,HERE)
   self.assertFalse(destination.exists())
""")
text=method(text,'test_r12_original_input_bytes_are_reopened',"""
 def test_r12_original_input_bytes_are_reopened(self):
  core=runtime.load(HERE/'scripts/provision-static-onnx-runtime.py','portable_gpu_file_owner')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   q=Path(t)/'source';q.write_bytes(b'original');row={'path':str(q),'bytes':8,'sha256':hashlib.sha256(b'original').hexdigest()};q.write_bytes(b'changed!')
   with self.assertRaises(ValueError):runtime._same(core,q,row)
""")
text=method(text,'test_checkpoint_missing_composite_refuses_without_outputs',"""
 def test_checkpoint_missing_composite_refuses_without_outputs(self):
  checkpoint=load('cuda-static-checkpointed')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   proposal=Path(t)/'proposal';proposal.mkdir();composite=Path(t)/'missing-composite'
   (proposal/'compose_checkpointed_build.py').write_bytes(b'inert composer')
   (proposal/'input-contract.json').write_bytes(b'{}')
   def plain(p,regular=False):
    p=Path(p)
    if regular and not p.is_file():raise FileNotFoundError(str(p))
    return p
   core=SimpleNamespace(plain=plain,digest=lambda p:checkpoint.COMPOSER if Path(p).name=='compose_checkpointed_build.py' else checkpoint.INPUT)
   fake_runtime=SimpleNamespace(load=lambda *args:core)
   with patch.object(checkpoint,'module',return_value=fake_runtime),self.assertRaises(FileNotFoundError):
    checkpoint.verify_checkpointed(HERE,composite,proposal,'0'*64,HERE)
   self.assertFalse(composite.exists())
""")
text=method(text,'test_actual_tool_probe_metadata_stays_outside_canonical_proof',"""
 def test_actual_tool_probe_metadata_stays_outside_canonical_proof(self):
  names=['compiler','cmake','ninja','patch','python','vcvars']
  tools={name:{'path':str(HERE/'literal-tools'/name),'bytes':i+1,'sha256':format(i+1,'064x'),'version':'literal-version','probe':{'elapsed_seconds':1.5,'passed':False}} for i,name in enumerate(names)}
  pins=load('cuda-static-checkpointed').toolchain_pins(tools)
  expected=[{'path':str(HERE/'literal-tools'/name),'bytes':i+1,'sha256':format(i+1,'064x')} for i,name in enumerate(['compiler','cmake','ninja','patch','python','vcvars'])]
  self.assertEqual(pins,expected)
  for row in pins:self.assertEqual(set(row),{'path','bytes','sha256'});self.assertIs(type(row['bytes']),int)
  self.assertNotIn('probe',str(pins));runtime.canonical(pins)
""")
text=text.replace("repo=Path('D:/code/Github/airk')","repo=HERE")
targets[name]=text.encode()
name='scripts/tests/test_native_package_adapter.py';text=targets[name].decode()
text=text.replace('import importlib.util,unittest,tempfile,hashlib','import importlib.util,unittest,tempfile,hashlib'+N+'from types import SimpleNamespace')
text=text.replace('class Controls(unittest.TestCase):','''class Controls(unittest.TestCase):
 def proof_paths(self):
  return {'projection':HERE/'literal-projection.json','native_emission':HERE/'literal-emission.json','stdout':HERE/'literal-stdout.log'}
 def proof_row(self,path):
  return {'path':str(path),'bytes':1,'sha256':'a'*64}
''')
text=method(text,'test_final_clock_follows_last_stdout_hash',"""
 def test_final_clock_follows_last_stdout_hash(self):
  events=[];proofs=self.proof_paths()
  def hash_file(path):events.append(path.name);return self.proof_row(path)
  value={'complete':True,'failure':None}
  n.finish_observation(value,0,10,proofs,clock=lambda:(events.append('clock') or 9),hash_file=hash_file)
  self.assertEqual(events,['literal-projection.json','literal-emission.json','literal-stdout.log','clock'])
  self.assertTrue(value['complete']);self.assertEqual(value['final_proof_success'],['projection','native_emission','stdout'])
""")
text=method(text,'test_exact_cutoff_refuses_completion',"""
 def test_exact_cutoff_refuses_completion(self):
  value={'complete':True,'failure':None}
  n.finish_observation(value,0,10,self.proof_paths(),clock=lambda:10,hash_file=self.proof_row)
  self.assertFalse(value['complete']);self.assertTrue(value['whole_phase_expired']);self.assertEqual(value['elapsed_seconds'],10)
""")
text=method(text,'test_original_error_survives_retention_error',"""
 def test_original_error_survives_retention_error(self):
  original={'type':'NativeError','message':'actual original'};value={'complete':False,'failure':original};events=[]
  def failure(path):events.append(path.name);raise OSError('actual retention error')
  n.finish_observation(value,0,10,self.proof_paths(),clock=lambda:(events.append('clock') or 9),hash_file=failure)
  self.assertEqual(value['failure'],original);self.assertEqual(value['retention_failure']['type'],'OSError')
  self.assertEqual(len(value['final_proof_errors']),3);self.assertEqual(events[-1],'clock')
""")
append="""
class MandatoryProofControls(unittest.TestCase):
 proof_paths=Controls.proof_paths
 proof_row=Controls.proof_row
 def test_skipped_missing_proof_cannot_pass_and_all_callbacks_finish_before_clock(self):
  for missing in ['projection','native_emission','stdout']:
   with self.subTest(missing=missing):
    value={'complete':True,'failure':None};proofs=self.proof_paths();events=[]
    def observed(path):
     events.append(path.name)
     return None if path==proofs[missing] else self.proof_row(path)
    n.finish_observation(value,0,10,proofs,clock=lambda:(events.append('clock') or 9),hash_file=observed)
    self.assertFalse(value['complete']);self.assertNotIn(missing,value['final_proof_success'])
    self.assertEqual(len(events),4);self.assertEqual(events[-1],'clock')
 def test_missing_label_and_malformed_sha_row_cannot_pass(self):
  for edit in ['label','sha','path','size']:
   with self.subTest(edit=edit):
    value={'complete':True,'failure':None};proofs=self.proof_paths()
    if edit=='label':proofs.pop('projection')
    def observed(path):
     row=self.proof_row(path)
     if edit=='sha':row.pop('sha256')
     if edit=='path':row['path']=str(HERE/'wrong-output')
     if edit=='size':row['bytes']=(64<<20)+1
     return row
    n.finish_observation(value,0,10,proofs,clock=lambda:9,hash_file=observed)
    self.assertFalse(value['complete']);self.assertTrue(value['final_proof_errors'])
 def test_final_drift_keeps_original_native_pin_and_failure(self):
  original={'type':'OriginalNativeFailure'};value={'complete':False,'failure':original};proofs=self.proof_paths()
  value['projection']={'path':str(proofs['projection']),'bytes':1,'sha256':'b'*64}
  n.finish_observation(value,0,10,proofs,clock=lambda:9,hash_file=self.proof_row)
  self.assertEqual(value['failure'],original);self.assertEqual(value['projection']['sha256'],'b'*64)
  self.assertEqual(value['final_proof_drift']['projection']['observed']['sha256'],'a'*64)
 def test_established_no_site_parent_roster_is_exact(self):
  expected={'reference_owned.py':'2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1','reference_parent_no_site_r2.py':'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d','reference_worker.py':'9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'}
  self.assertEqual(n.HELPERS,expected)
  parent=SimpleNamespace(worker=SimpleNamespace(__file__=HERE/'reference_worker.py'));loaded=[]
  def load(path,name):loaded.append(Path(path).name);return parent if 'parent' in Path(path).name else object()
  with patch.object(n.sys,'path',list(n.sys.path)),patch.object(n,'regular',side_effect=Path),patch.object(n,'digest',side_effect=lambda p:expected[Path(p).name]),patch.object(n,'load',side_effect=load):
   n.helpers({'ownership_helper_root':str(HERE)})
  self.assertEqual(loaded,['reference_parent_no_site_r2.py','reference_owned.py'])
 def test_wrong_parent_pin_refuses_before_any_load(self):
  with patch.object(n,'regular',side_effect=Path),patch.object(n,'digest',return_value='0'*64),patch.object(n,'load') as loader:
   with self.assertRaisesRegex(ValueError,'ownership primitive'):n.helpers({'ownership_helper_root':str(HERE)})
   loader.assert_not_called()

"""
text=text.replace("if __name__=='__main__':unittest.main()",append+"if __name__=='__main__':unittest.main()")
targets[name]=text.encode()
for name,raw in targets.items():
    if name.endswith('.py'):ast.parse(raw.decode('utf-8'),filename=name)
for name,raw in targets.items():
    path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
    with path.open('wb' if name in before else 'xb') as stream:stream.write(raw)
manifest=[{'path':name,'sha256':h(raw),'bytes':len(raw),'existing':name in before} for name,raw in sorted(targets.items())]
(app/'applied-source-map.json').write_text(json.dumps(manifest,indent=2)+N,encoding='utf-8')
print('Applied',len(targets),'maintained files; all Python syntax parsed, no native/Cargo/model.')