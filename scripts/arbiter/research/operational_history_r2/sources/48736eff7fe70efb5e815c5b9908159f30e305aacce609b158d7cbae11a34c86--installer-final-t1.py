# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[
 [sys.executable,'scripts/dev-check.py','--workspace','--all-features','--target-dir','artifacts/v190/p10-target'],
 ['cargo','check','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings'],
 [sys.executable,'scripts/dev/boundary_check.py'],
 [sys.executable,'scripts/dev/d6_archive_docs.py','--check'],
 [sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'],
 ['pwsh','-NoProfile','-File','scripts/check-repo.ps1'],
 ['git','-c','safe.directory=D:/code/Github/airk','diff','--check'],
]
for command in commands:
 print('COMMAND',command,flush=True)
 result=subprocess.run(command)
 if result.returncode:sys.exit(result.returncode)
