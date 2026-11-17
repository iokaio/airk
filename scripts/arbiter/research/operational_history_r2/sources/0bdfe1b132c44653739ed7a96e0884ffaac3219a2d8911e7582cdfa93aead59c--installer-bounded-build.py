# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
commands=[
 ['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--lib','ordinary_same_runtime_reconnect_does_not_inspect_missing_custom_storage','--','--test-threads=1'],
 ['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--bin','airk','cli_reference_documents_every_public_command','--','--test-threads=1'],
 [sys.executable,'scripts/build-service-runtime.py','--profile','debug','--destination','artifacts/one-process/bounded-runtime-1823'],
 [sys.executable,'scripts/package-native.py','--target','x86_64-pc-windows-msvc','--runtime-dir','artifacts/one-process/bounded-runtime-1823','--output','artifacts/one-process/bounded-native-1823'],
 [sys.executable,'scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir','artifacts/one-process/bounded-native-1823/publish','--output','artifacts/one-process/bounded-extension-1823'],
]
for command in commands:
 print('COMMAND',command,flush=True)
 env=os.environ.copy()
 if 'cli_reference_documents_every_public_command' in command:env['AIRK_WRITE_CLI_REFERENCE']='1'
 result=subprocess.run(command,env=env)
 if result.returncode:sys.exit(result.returncode)
