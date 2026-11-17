# SPDX-License-Identifier: Apache-2.0
"""Focused script modules using the repository's flat unittest-discovery layout."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path.cwd() / 'scripts/tests'))
suite = unittest.defaultTestLoader.loadTestsFromNames([
    'test_current_model_consumers', 'test_benchmark_service_http', 'test_service_clients',
])
result = unittest.TextTestRunner(verbosity=2, failfast=True).run(suite)
sys.exit(0 if result.wasSuccessful() else 1)
