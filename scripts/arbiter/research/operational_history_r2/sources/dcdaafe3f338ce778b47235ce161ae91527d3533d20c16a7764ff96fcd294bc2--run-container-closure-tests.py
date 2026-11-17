# SPDX-License-Identifier: Apache-2.0
import sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path("scripts/tests").resolve()))
loader=unittest.TestLoader()
pending={"test_all_external_rust_test_inputs_are_exact_manifest_files","test_actual_multistage_repository_recipe_has_a_bounded_complete_source_seal"}
def cases(suite):
 for test in suite:
  if isinstance(test,unittest.TestSuite):yield from cases(test)
  else:yield test
suite=unittest.TestSuite(test for name in ("test_container_inputs","test_container_runtime_build") for test in cases(loader.loadTestsFromName(name)) if test._testMethodName not in pending)
print("Explicit scope: two production-catalog-dependent checks remain PENDING, not selected or passed.",flush=True)
result=unittest.TextTestRunner(verbosity=2,failfast=True).run(suite)
raise SystemExit(not result.wasSuccessful())
