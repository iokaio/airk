# SPDX-License-Identifier: Apache-2.0
"""Private explicit residual selection; required full gate remains incomplete."""
import hashlib
import json
from pathlib import Path
import sys
import unittest

ROOT = Path.cwd()
OUT = Path(__file__).resolve().parent
excluded = {
    'test_container_inputs.ContainerInputInventoryTests.test_all_external_rust_test_inputs_are_exact_manifest_files': {
        'reason': 'Required genuine fixtures/arbiter/publication_catalog.json absent; qualification cannot be fabricated.',
        'failure_receipt': 'artifacts/advanced-split/receipts/python-container-closure-r2/container-closure.json',
        'failure_log': 'artifacts/advanced-split/receipts/python-container-closure-r2/container-closure.log',
    },
}
for value in excluded.values():
    value['failure_receipt_sha256'] = hashlib.sha256((ROOT/value['failure_receipt']).read_bytes()).hexdigest()
    value['failure_log_sha256'] = hashlib.sha256((ROOT/value['failure_log']).read_bytes()).hexdigest()

def flatten(suite):
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from flatten(item)
        else:
            yield item

loader = unittest.TestLoader()
tests = list(flatten(loader.discover(str(ROOT/'scripts/tests'))))
ids = [item.id() for item in tests]
assert set(excluded) <= set(ids), 'Every excluded known failure must be in actual discovery'
assert len(ids) == len(set(ids)), 'Discovery has duplicated identities'
selected = [item for item in tests if item.id() not in excluded]
record = {'schema': 1, 'full_required_gate': 'failed/incomplete', 'discovered_ids': ids,
          'selected_ids': [item.id() for item in selected], 'excluded': excluded}
(OUT/'selection.json').write_text(json.dumps(record, indent=2)+'\n', encoding='utf8')
print(json.dumps({'discovered_count': len(tests), 'selected_count': len(selected), 'excluded_count': len(excluded)}), flush=True)
result = unittest.TextTestRunner(verbosity=2, failfast=True).run(unittest.TestSuite(selected))
record.update(tests_run=result.testsRun,failures=[test.id() for test,_ in result.failures],errors=[test.id() for test,_ in result.errors],skips=[{'id':test.id(),'reason':reason}for test,reason in result.skipped],residual_success=result.wasSuccessful())
(OUT/'selection.json').write_text(json.dumps(record, indent=2)+'\n', encoding='utf8')
sys.exit(0 if result.wasSuccessful() else 1)
