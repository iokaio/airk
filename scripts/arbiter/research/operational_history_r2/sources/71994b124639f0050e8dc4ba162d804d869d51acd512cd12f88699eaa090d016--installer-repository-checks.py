# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[
 [sys.executable,'scripts/dev/d6_archive_docs.py','--check'],
 [sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'],
 ['pwsh','-NoProfile','-File','scripts/check-repo.ps1'],
 ['git','-c','safe.directory=D:/code/Github/airk','diff','--check'],
 [sys.executable,'scripts/dependency-audit.py','--offline','--output','artifacts/one-process/installer-1823-dependency-audit.json'],
]
failures=[]
for command in commands:
 print('COMMAND',command,flush=True)
 result=subprocess.run(command)
 if result.returncode:failures.append((command,result.returncode))
print('COLLECTED FAILURES',failures,flush=True)
sys.exit(1 if failures else 0)
