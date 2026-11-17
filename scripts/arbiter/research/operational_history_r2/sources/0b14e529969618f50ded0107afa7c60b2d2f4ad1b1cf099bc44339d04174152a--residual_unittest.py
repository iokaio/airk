# SPDX-License-Identifier: Apache-2.0
"""Full residual Python selection in the normal owner context; full gate remains blocked."""
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

ROOT=Path.cwd();OUT=Path(__file__).resolve().parent
prior=ROOT/'artifacts/advanced-split/receipts/python-residual-continuation-r5/selection.json'
old=json.loads(prior.read_bytes());excluded=old['excluded']
assert len(excluded)==3
assert not (ROOT/'fixtures/arbiter/publication_catalog.json').exists(), 'Publication prerequisite changed; review exclusions again'
for value in excluded.values():
 for key in ['failure_receipt','failure_log']:
  if key in value:assert hashlib.sha256((ROOT/value[key]).read_bytes()).hexdigest()==value[key+'_sha256']
identity=subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip()
identity_row=next(csv.reader([identity]));assert identity_row[-1].endswith('-1002'), 'Normal checkout owner required for actual Windows SSPI'

def flatten(suite):
 for item in suite:
  if isinstance(item,unittest.TestSuite):yield from flatten(item)
  else:yield item

tests=list(flatten(unittest.TestLoader().discover(str(ROOT/'scripts/tests'))));ids=[test.id()for test in tests]
assert len(ids)==len(set(ids)) and set(excluded)<=set(ids)
assert ids==old['discovered_ids'], 'Discovery changed; review this complete residual selection'
selected=[test for test in tests if test.id()not in excluded]
record={'schema':1,'full_required_gate':'blocked: three genuine-publication prerequisite tests remain unexecuted',
        'execution_identity':{'name':identity_row[0],'sid':identity_row[-1]},'fresh_owned_fixtures':True,
        'discovered_ids':ids,'selected_ids':[test.id()for test in selected],'reused_passed_ids':[],
        'excluded':excluded,'prior_selection_sha256':hashlib.sha256(prior.read_bytes()).hexdigest()}
(OUT/'selection.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'discovered_count':len(tests),'selected_count':len(selected),'excluded_count':len(excluded),'execution_identity':record['execution_identity']}),flush=True)
result=unittest.TextTestRunner(verbosity=2,failfast=True).run(unittest.TestSuite(selected))
record.update(tests_run=result.testsRun,failures=[test.id()for test,_ in result.failures],errors=[test.id()for test,_ in result.errors],skips=[{'id':test.id(),'reason':reason}for test,reason in result.skipped],residual_success=result.wasSuccessful())
(OUT/'selection.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
sys.exit(0 if result.wasSuccessful()else 1)
