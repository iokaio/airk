# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[
 ['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--test','cli_commands','replacement','--','--test-threads=1'],
 ['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--bin','airk','cli_reference_documents_every_public_command','--','--test-threads=1'],
]
for command in commands:
 print('COMMAND',command,flush=True)
 result=subprocess.run(command)
 if result.returncode:sys.exit(result.returncode)
