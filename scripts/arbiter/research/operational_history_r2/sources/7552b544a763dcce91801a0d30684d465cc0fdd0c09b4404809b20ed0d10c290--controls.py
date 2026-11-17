# SPDX-License-Identifier: Apache-2.0
"""Private fixture-owner candidate controls; public files remain untouched."""
from pathlib import Path
import importlib.util,sys,unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT))
name='scripts.tests.test_github_release';path=ROOT/'scripts/tests/test_github_release.py'
spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);sys.modules[name]=module
exec(compile(Path(__file__).with_name('test_github_release.py').read_bytes(),str(path),'exec'),module.__dict__)
suite=unittest.TestSuite([unittest.defaultTestLoader.loadTestsFromModule(module),unittest.defaultTestLoader.loadTestsFromName('scripts.tests.test_optional_assembly')])
result=unittest.TextTestRunner(verbosity=2,failfast=True).run(suite);raise SystemExit(0 if result.wasSuccessful() else 1)
