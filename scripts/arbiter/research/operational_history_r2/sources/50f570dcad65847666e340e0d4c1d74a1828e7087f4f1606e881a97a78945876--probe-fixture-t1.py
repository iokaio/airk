# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
source = (root/'examples/local_https_client.ps1').read_text()
source = source.replace("[Console]::Error.WriteLine('AIRK local HTTPS read failed; obtain current verified connection metadata and inspect native status.')", "$airkProbeException=$_.Exception; while($airkProbeException){[Console]::Error.WriteLine($airkProbeException.GetType().FullName + ': ' + $airkProbeException.Message);$airkProbeException=$airkProbeException.InnerException}")
probe = base/'probe-fixture-t1.ps1'
probe.write_text(source)
spec = importlib.util.spec_from_file_location('probe_https_example',root/'scripts/tests/test_local_https_examples.py')
module = importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
test = module.LocalHttpsExamplesTests('test_native_windows_namespace_file_references')
test.setUpClass()
try:
    with test.server() as (connection,observed):
        result = test.run_client(('pwsh',[module.PWSH,'-NoProfile','-File',str(probe)],['-Connection','-Tool','-Arguments']),connection)
        print('exit:',result.returncode,'requests:',len(observed))
        print(result.stderr)
finally:
    test.tearDownClass()
