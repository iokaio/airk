# SPDX-License-Identifier: Apache-2.0
"""Review controls for the private exact-native fixture and test candidates."""
from pathlib import Path
import importlib.util,sys,unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(ROOT))
source=ROOT/'scripts/tests/test_current_model_consumers.py'
spec=importlib.util.spec_from_file_location('private_native_smoke_controls',source);module=importlib.util.module_from_spec(spec)
original=Path.read_bytes;fixture=ROOT/'fixtures/models/native-smoke-observations.json'
def read(path):return original(HERE/'native-smoke-observations.json') if path==fixture else original(path)
with patch.object(Path,'read_bytes',read):exec(compile((HERE/'test_current_model_consumers.py').read_bytes(),str(source),'exec'),module.__dict__)
result=unittest.TextTestRunner(verbosity=2,failfast=True).run(unittest.defaultTestLoader.loadTestsFromModule(module));raise SystemExit(0 if result.wasSuccessful() else 1)
