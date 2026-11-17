# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[
 ['cargo','build','--workspace','--locked','--timings'],
 ['cargo','nextest','run','--workspace','--all-features','--locked','--timings','--profile','ci','--ignore-default-filter','--no-fail-fast'],
 ['cargo','test','--doc','--workspace','--all-features','--locked','--timings','--','--test-threads=1'],
]
for command in commands:
 print('COMMAND',command,flush=True)
 result=subprocess.run(command)
 if result.returncode:sys.exit(result.returncode)
