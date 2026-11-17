# SPDX-License-Identifier: Apache-2.0
"""Exact full current Python catalog less three retained genuine-input gates."""
import hashlib
import json
from pathlib import Path
import sys
import time
import unittest

ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'artifacts/advanced-split/receipts/full-script-r101'
sys.path.insert(0,str(ROOT))
EXCLUDED={
 'test_container_inputs.ContainerInputInventoryTests.test_all_external_rust_test_inputs_are_exact_manifest_files',
 'test_container_runtime_build.ContainerSourceSealTests.test_actual_multistage_repository_recipe_has_a_bounded_complete_source_seal',
 'test_package_extension.VsixContracts.test_actual_compiled_runtime_imports_are_all_staged',
}
def flatten(suite):
 for item in suite:
  if isinstance(item,unittest.TestSuite):yield from flatten(item)
  else:yield item
def main():
 tests=list(flatten(unittest.defaultTestLoader.discover(str(ROOT/'scripts/tests'),pattern='test*.py')))
 ids=[test.id() for test in tests]
 assert len(ids)==len(set(ids)) and EXCLUDED.issubset(ids)
 prior=ROOT/'artifacts/advanced-split/receipts/python-residual-continuation-r5/selection.json'
 original=json.loads(prior.read_bytes())
 assert set(original['excluded'])==EXCLUDED
 selected=[test for test in tests if test.id() not in EXCLUDED]
 OUT.mkdir(parents=True,exist_ok=True)
 selection={'discovered_ids':ids,'selected_ids':[t.id() for t in selected],
  'excluded':original['excluded'],'original_selection_sha256':hashlib.sha256(prior.read_bytes()).hexdigest(),
  'required_deferred_gate':'Run these three unchanged after genuine qualified native Arbiter publication inputs exist; not qualified now.'}
 (OUT/'python-selection.json').write_text(json.dumps(selection,indent=2)+'\n')
 started=time.monotonic()
 result=unittest.TextTestRunner(verbosity=2,failfast=True).run(unittest.TestSuite(selected))
 data={'selected':len(selected),'executed':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),
  'skipped':[{'id':t.id(),'reason':why} for t,why in result.skipped],
  'elapsed_seconds':time.monotonic()-started,'successful':result.wasSuccessful(),
  'unexecuted':len(selected)-result.testsRun,'deferred':sorted(EXCLUDED)}
 (OUT/'python-outcome.json').write_text(json.dumps(data,indent=2)+'\n')
 print(json.dumps(data),flush=True)
 return 0 if result.wasSuccessful() and result.testsRun==len(selected) else 1
if __name__=='__main__':raise SystemExit(main())
