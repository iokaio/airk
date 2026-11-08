# SPDX-License-Identifier: Apache-2.0
"""Historical fixture routing only; never invokes Service Management."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('historical_mac_login', Path(__file__).parents[1]/'check-macos-login.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class HistoricalMacLoginTests(unittest.TestCase):
    def test_guard_preserves_actual_14_input_and_rejects_current_or_unqualified_eras(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            metadata = root/'airk-runtime.json'
            metadata.write_text(json.dumps({'schema':1,'version':'1.4.4'}))
            driver.require_historical_runtime(root)
            for schema, version in [(2,'1.8.14'),(1,'1.8.14'),(1,'1.8.12'),(2,'1.4.4')]:
                metadata.write_text(json.dumps({'schema':schema,'version':version}))
                with self.assertRaisesRegex(ValueError,'historical_fixture_only'):
                    driver.require_historical_runtime(root)

    def test_current_payload_refuses_before_fixture_creation_or_native_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'airk-runtime.json').write_text(json.dumps({'schema':2,'version':'1.8.14'}))
            before = list(root.iterdir())
            with patch.object(driver.platform,'system',return_value='Darwin'), \
                 patch.object(driver.subprocess,'run',side_effect=AssertionError('Native command must not run')), \
                 patch('sys.argv',['check-macos-login.py','--runtime',str(root),'--approved-sha256','0'*64]):
                with self.assertRaisesRegex(ValueError,'historical_fixture_only'):
                    driver.main()
            self.assertEqual(list(root.iterdir()),before)


if __name__ == '__main__':
    unittest.main()
