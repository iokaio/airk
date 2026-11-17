# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import ast,hashlib,json,sys,time,unittest
BASE=Path(__file__).resolve().parent
OUT=BASE/'cheap-validation-r3'
assert not OUT.exists()
OUT.mkdir()
sys.path.insert(0,str(BASE))
modules=['test_public_focus_r5','test_outer_r5','test_native_singleton_r5']
loader=unittest.TestLoader();suite=loader.loadTestsFromNames(modules)
def ids(s):
 for t in s:
  if isinstance(t,unittest.TestSuite):yield from ids(t)
  else:yield t.id()
selected=list(ids(suite))
inputs=list(BASE.glob('*.py'))+list((BASE/'public-focus-r5-helper-source/scripts').glob('*.py'))+[BASE.parent.parent/'public-focus-native-r4/008-raw-diagnostics-a.json',BASE.parent.parent/'public-focus-native-r4/007-pre-read-source-oracles.json']
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
pins=[{'path':str(p),'sha256':sha(p)} for p in inputs]
for p in BASE.glob('*.py'):ast.parse(p.read_text(encoding='utf-8'),filename=str(p))
began=time.monotonic()
with (OUT/'stdout.txt').open('x',encoding='utf-8') as out,(OUT/'stderr.txt').open('x',encoding='utf-8') as err:
 oldout,olderr=sys.stdout,sys.stderr
 try:
  sys.stdout=out;sys.stderr=err
  result=unittest.TextTestRunner(stream=err,verbosity=2).run(suite)
 finally:sys.stdout,sys.stderr=oldout,olderr
receipt={'schema':'airk.private-python-controls/1','argv':sys.argv,'working_directory':str(BASE),'selected_modules':modules,'selected_test_ids':selected,'selected_count':len(selected),'tests_run':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),'passed':result.wasSuccessful() and result.testsRun==len(selected)>0,'elapsed_seconds':time.monotonic()-began,'input_pins':pins,'input_pins_unchanged':all(sha(Path(r['path']))==r['sha256'] for r in pins),'stdout_sha256':sha(OUT/'stdout.txt'),'stderr_sha256':sha(OUT/'stderr.txt'),'actual_service_started':False,'NN_invocations':0}
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:receipt[k] for k in ('passed','tests_run','failures','errors','elapsed_seconds')}))
if not receipt['passed']:print((OUT/'stderr.txt').read_text(encoding='utf-8'))
raise SystemExit(0 if receipt['passed'] else 1)
