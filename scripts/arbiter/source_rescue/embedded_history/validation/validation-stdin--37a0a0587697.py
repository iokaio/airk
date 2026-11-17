# SPDX-License-Identifier: Apache-2.0
import unittest,sys
sys.path.insert(0,'scripts/tests')
suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName(name) for name in ('test_mcp_investigation_benchmark','test_local_test_setup','test_runtime_upgrade'))
result=unittest.TextTestRunner(verbosity=1).run(suite)
sys.exit(not result.wasSuccessful())