# SPDX-License-Identifier: Apache-2.0
"""Retained-binary diagnostic only; not qualification of modified source."""
import hashlib,json,os,subprocess,time
from pathlib import Path
root=Path.cwd(); out=root/'artifacts/readiness-1817'
bins=[(root/'target/debug/deps/service_runtime-cadd3f6474c6bb3c.exe',['installed_','--nocapture','--test-threads=1']), (root/'target/debug/deps/cli_commands-db4189c7f37800cb.exe',['every_public_command_has_a_test_and_prints_help','--exact','--nocapture'])]
results=[]
for binary,args in bins:
    name=binary.name.split('-')[0]; cmd=[str(binary),*args]
    seal={str(p.relative_to(root)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in (binary,root/'target/debug/airk.exe')}
    start=time.monotonic()
    with (out/(name+'-diagnostic.log')).open('w',encoding='utf-8') as log:
        result=subprocess.run(cmd,cwd=root/'crates/airk-cli',stdout=log,stderr=subprocess.STDOUT,env=os.environ.copy())
    row={'kind':'retained compiled-artifact diagnostic; compiled-source identity not established','command':cmd,'input_sha256':seal,'elapsed_seconds':time.monotonic()-start,'exit_code':result.returncode,'log':str(out/(name+'-diagnostic.log'))}
    results.append(row); print(json.dumps(row),flush=True)
(out/'fixture-diagnostic.json').write_text(json.dumps(results,indent=2)+'\n')
