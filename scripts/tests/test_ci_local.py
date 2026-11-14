# SPDX-License-Identifier: Apache-2.0
"""Run the actual PowerShell loop with inert tools to prove failure stops the batch."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]

@unittest.skipUnless(os.name=='nt' and shutil.which('pwsh'), 'Windows PowerShell orchestration')
class LocalCiTests(unittest.TestCase):
    def invoke(self, failure):
        with tempfile.TemporaryDirectory() as temporary:
            owner=Path(temporary)
            runtime=owner/'runtime';runtime.mkdir();(runtime/'airk.exe').write_bytes(b'inert test file')
            script=owner/'invoke.ps1'
            script.write_text('''$ErrorActionPreference='Stop'
function global:python {
    [IO.File]::AppendAllText($env:AIRK_TEST_CALLS, (($args | ConvertTo-Json -Compress) + [Environment]::NewLine))
    $global:LASTEXITCODE = [int]$env:AIRK_TEST_FAILURE
}
& (Join-Path $env:AIRK_TEST_REPO 'scripts/ci-local.ps1') -Cores 1 -Repeat 2 -Suites smoke,clients,install -SkipBuild -RuntimeDir $env:AIRK_TEST_RUNTIME
exit $LASTEXITCODE
''')
            calls=owner/'calls.jsonl'
            env={**os.environ,'AIRK_TEST_CALLS':str(calls),'AIRK_TEST_FAILURE':str(failure),
                 'AIRK_TEST_REPO':str(ROOT),'AIRK_TEST_RUNTIME':str(runtime)}
            result=subprocess.run(['pwsh','-NoProfile','-File',str(script)],env=env,capture_output=True,text=True,timeout=30)
            observed=[json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
            return result,observed
    def test_first_failure_stops_suites_and_later_iterations_retaining_summary(self):
        result,calls=self.invoke(23)
        self.assertEqual(result.returncode,1,result.stdout+result.stderr)
        self.assertEqual(len(calls),1)
        self.assertIn('--cold-clients-only',calls[0])
        self.assertIn('0/1 passed',result.stdout)
    def test_success_runs_every_native_owner_without_extension_build(self):
        result,calls=self.invoke(0)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertEqual(len(calls),6)
        self.assertEqual([call[0] for call in calls],['scripts/check-service-clients.py','scripts/check-service-clients.py','scripts/check-service-install.py']*2)
        self.assertIn('6/6 passed',result.stdout)

if __name__=='__main__': unittest.main()
