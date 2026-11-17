# SPDX-License-Identifier: Apache-2.0
"""Continue only the unexecuted suffix; preserve prior failure as required open work."""
import hashlib
import json
from pathlib import Path
import sys
import time
import unittest
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'artifacts/advanced-split/receipts/full-script-r101'
sys.path.insert(0,str(ROOT))
FAILED='test_optional_assembly.OptionalAssembly.test_four_target_assembly_keeps_optional_parts_separate_and_binds_all_published_bytes'
def flatten(suite):
 for item in suite:
  if isinstance(item,unittest.TestSuite):yield from flatten(item)
  else:yield item
def main():
 full=json.loads((OUT/'python-selection.json').read_bytes())
 prior=json.loads((OUT/'python-continuation-selection.json').read_bytes())
 outcome=json.loads((OUT/'python-continuation-outcome.json').read_bytes())
 tests=list(flatten(unittest.defaultTestLoader.discover(str(ROOT/'scripts/tests'),pattern='test*.py')))
 ids=[test.id() for test in tests]
 assert ids==full['discovered_ids']
 selected=prior['selected_ids'];position=selected.index(FAILED)
 assert outcome['executed']==position+1 and outcome['errors']==1 and not outcome['failures']
 remaining=selected[position+1:];assert len(remaining)==outcome['unexecuted']
 lookup={test.id():test for test in tests}
 proof={'selected_ids':remaining,'original_selection_sha256':hashlib.sha256((OUT/'python-continuation-selection.json').read_bytes()).hexdigest(),
  'prior_outcome_sha256':hashlib.sha256((OUT/'python-continuation-outcome.json').read_bytes()).hexdigest(),
  'open_required_failed_case':FAILED,'prefix_not_repeated':selected[:position],
  'not_a_qualification_waiver':True}
 (OUT/'python-continuation2-selection.json').write_text(json.dumps(proof,indent=2)+'\n')
 started=time.monotonic();result=unittest.TextTestRunner(verbosity=2,failfast=True).run(unittest.TestSuite(lookup[name] for name in remaining))
 data={'selected':len(remaining),'executed':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),
  'skipped':[{'id':t.id(),'reason':why} for t,why in result.skipped],
  'elapsed_seconds':time.monotonic()-started,'successful':result.wasSuccessful(),
  'unexecuted':len(remaining)-result.testsRun,'prior_required_failure_still_open':FAILED}
 (OUT/'python-continuation2-outcome.json').write_text(json.dumps(data,indent=2)+'\n');print(json.dumps(data),flush=True)
 return 0 if result.wasSuccessful() and result.testsRun==len(remaining) else 1
if __name__=='__main__':raise SystemExit(main())
