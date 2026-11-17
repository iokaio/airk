# SPDX-License-Identifier: Apache-2.0
"""Private residual continuation; no full-gate claim or changed assertion."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import unittest

ROOT=Path.cwd();OUT=Path(__file__).resolve().parent
prior=ROOT/'artifacts/advanced-split/receipts/python-residual-selection-r4'
receipt=json.loads((prior/'residual-python.json').read_bytes())
selection=json.loads((prior/'selection.json').read_bytes())
spec=importlib.util.spec_from_file_location('receipt_input_owner',ROOT/'scripts/dev/receipt_runner.py')
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
current=owner.observe(list(receipt['groups']))
assert receipt['inputs_unchanged'] and not receipt['changed_groups']
assert {key:value['sha256'] for key,value in current.items()}=={key:value['sha256'] for key,value in receipt['groups'].items()},'Source drift forbids prefix evidence reuse'
assert selection['tests_run']==223 and len(selection['errors'])==1 and not selection['failures'] and not selection['skips']
reused=selection['selected_ids'][:222]
assert selection['selected_ids'][222]==selection['errors'][0]
excluded=dict(selection['excluded'])
excluded[selection['errors'][0]]={'reason':'Same missing qualified publication capture is the only absent source-stage COPY input.','failure_receipt':str((prior/'residual-python.json').relative_to(ROOT)),'failure_log':str((prior/'residual-python.log').relative_to(ROOT)),'source_trace':['scripts/tests/test_container_runtime_build.py:62 -> build-service-container.py::container_source_identity','Docker COPY census: only missing fixtures/arbiter/publication_catalog.json']}
excluded['test_package_extension.VsixContracts.test_actual_compiled_runtime_imports_are_all_staged']={'reason':'Actual compiled module staging calls mandatory native publication bootstrap verifier; no capture exists.','failure_receipt':'artifacts/advanced-split/receipts/optional-publication-prerequisite-audit/actual-gate-refusal.json','source_trace':['scripts/tests/test_package_extension.py:572 -> package-extension.py::stage_runtime_modules','package-extension.py::verify_bootstrap_release_runtime -> build-bootstrap.cjs::verifyReleaseBundle(required=true) -> nativeProjection(true)','Only missing production input fixtures/arbiter/publication_catalog.json; no test has been mocked or rewritten']}
for value in excluded.values():
 for key in ['failure_receipt','failure_log']:
  if key in value:value[key+'_sha256']=hashlib.sha256((ROOT/value[key]).read_bytes()).hexdigest()
def flatten(suite):
 for item in suite:
  if isinstance(item,unittest.TestSuite):yield from flatten(item)
  else:yield item
tests=list(flatten(unittest.TestLoader().discover(str(ROOT/'scripts/tests'))));ids=[item.id() for item in tests]
assert ids==selection['discovered_ids'],'Discovery changed; cannot reuse prior identities'
assert set(excluded)<=set(ids) and not set(reused)&set(excluded)
selected=[item for item in tests if item.id() not in set(reused)|set(excluded)]
record={'schema':1,'full_required_gate':'failed/incomplete','discovered_ids':ids,'selected_ids':[item.id() for item in selected],'reused_passed_ids':reused,'reused_receipt':str((prior/'residual-python.json').relative_to(ROOT)),'reused_receipt_sha256':hashlib.sha256((prior/'residual-python.json').read_bytes()).hexdigest(),'current_group_sha256':{key:value['sha256']for key,value in current.items()},'reused_groups_equal':True,'excluded':excluded}
(OUT/'selection.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf8')
print(json.dumps({'discovered_count':len(tests),'selected_count':len(selected),'reused_pass_count':len(reused),'excluded_count':len(excluded)}),flush=True)
result=unittest.TextTestRunner(verbosity=2,failfast=True).run(unittest.TestSuite(selected))
record.update(tests_run=result.testsRun,failures=[test.id()for test,_ in result.failures],errors=[test.id()for test,_ in result.errors],skips=[{'id':test.id(),'reason':reason}for test,reason in result.skipped],residual_success=result.wasSuccessful())
(OUT/'selection.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf8')
sys.exit(0 if result.wasSuccessful() else 1)
