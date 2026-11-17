# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[['pwsh','-NoProfile','-File','scripts/check-repo.ps1'],['python','scripts/release-version.py','check','--expected','1.8.16'],[r'C:\Users\tdotn\.cargo\bin\cargo.exe','fmt','--all','--check'],['git','-c','safe.directory=D:/code/Github/airk','-c','core.safecrlf=false','diff','--check']]
for command in commands:
 result=subprocess.run(command)
 if result.returncode: sys.exit(result.returncode)